//! # Outbound-destination guard for host-mediated egress
//!
//! Every `/host/*` route that lets a caller reach the network on the host's
//! behalf (`/host/http`, `/host/udp/*`) is an SSRF primitive: the caller runs
//! in a browser (or a WASM guest) that cannot itself address `127.0.0.1`,
//! `169.254.169.254`, or a LAN `192.168.x.x`, but the host can. Without a
//! filter the bridge hands that reach to whoever can POST to it.
//!
//! This module is the single place that decides whether a destination is
//! allowed. It is deliberately **default-deny** for loopback / private /
//! link-local / multicast / reserved addresses, with one documented escape
//! hatch for local development against the mock backends in `servers/`:
//!
//! ```text
//! POLY_ALLOW_PRIVATE_NETWORK=1
//! ```
//!
//! Set that only on a developer machine or in the test harness — it re-opens
//! the SSRF surface by design.
//!
//! ## WASM safety
//!
//! Native-only: the guard resolves DNS via `tokio::net::lookup_host`, which
//! does not exist on `wasm32-unknown-unknown`. WASM callers never reach this
//! code — they talk to the native half over HTTP, and the native half applies
//! the guard.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

/// Environment variable that re-enables private / loopback destinations.
pub const ALLOW_PRIVATE_ENV: &str = "POLY_ALLOW_PRIVATE_NETWORK";

/// Whether host-mediated egress may target private / loopback destinations.
///
/// Injected rather than read from the environment at every call site so
/// tests (and any embedder that wants a different rule) can pick the policy
/// explicitly instead of mutating process state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PrivateNetworkPolicy {
    /// Reject loopback, private, link-local, multicast and reserved ranges.
    #[default]
    Deny,
    /// Allow every routable-or-not destination. Local development only.
    Allow,
}

impl PrivateNetworkPolicy {
    /// Read the policy from [`ALLOW_PRIVATE_ENV`]. Anything other than
    /// `1` / `true` (case-insensitive) means [`PrivateNetworkPolicy::Deny`].
    #[must_use]
    pub fn from_env() -> Self {
        match std::env::var(ALLOW_PRIVATE_ENV) {
            Ok(raw) if raw == "1" || raw.eq_ignore_ascii_case("true") => Self::Allow,
            Ok(_) | Err(_) => Self::Deny,
        }
    }

    /// `true` when the policy blocks non-public destinations.
    #[must_use]
    pub const fn denies_private(self) -> bool {
        matches!(self, Self::Deny)
    }
}

/// `true` when `ip` is not a public, routable unicast address.
#[must_use]
pub fn is_blocked_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_blocked_v4(v4),
        IpAddr::V6(v6) => is_blocked_v6(v6),
    }
}

fn is_blocked_v4(ip: Ipv4Addr) -> bool {
    if ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_multicast()
        || ip.is_broadcast()
        || ip.is_unspecified()
        || ip.is_documentation()
    {
        return true;
    }
    let [a, b, _, _] = ip.octets();
    // 100.64.0.0/10 — carrier-grade NAT shared space (RFC 6598).
    if a == 100 && (64..=127).contains(&b) {
        return true;
    }
    // 0.0.0.0/8 ("this network") and 240.0.0.0/4 (reserved / 255.x).
    a == 0 || a >= 240
}

fn is_blocked_v6(ip: Ipv6Addr) -> bool {
    // `::ffff:a.b.c.d` reaches the same host an IPv4 literal would.
    if let Some(mapped) = ip.to_ipv4_mapped() {
        return is_blocked_v4(mapped);
    }
    if ip.is_loopback() || ip.is_multicast() || ip.is_unspecified() {
        return true;
    }
    let [first, ..] = ip.segments();
    // fc00::/7 unique-local, fe80::/10 link-local.
    (first & 0xfe00) == 0xfc00 || (first & 0xffc0) == 0xfe80
}

/// Reject `addr` when `policy` denies private destinations.
///
/// # Errors
///
/// Returns a human-readable reason when the destination is blocked.
pub fn check_socket_addr(addr: SocketAddr, policy: PrivateNetworkPolicy) -> Result<(), String> {
    if policy.denies_private() && is_blocked_ip(addr.ip()) {
        return Err(format!(
            "destination {addr} is a loopback/private/reserved address; \
             set {ALLOW_PRIVATE_ENV}=1 to allow it"
        ));
    }
    Ok(())
}

/// Validate an outbound HTTP(S) URL: scheme allowlist plus a destination
/// check on **every** address the host name resolves to.
///
/// Resolving here (rather than trusting the literal) is what stops an
/// attacker-controlled `evil.test A 127.0.0.1` record. It is not a defence
/// against true DNS rebinding — the connect that follows resolves again, and
/// a short-TTL record can change between the two — but it closes the
/// single-lookup case, which is the one that is trivial to exploit.
///
/// Callers that follow redirects must re-run this for **each** hop; a guard
/// applied only to the first URL is bypassed by a `302` to `127.0.0.1`.
///
/// # Errors
///
/// Returns a reason when the scheme is not `http`/`https`, the URL has no
/// host, DNS fails, or any resolved address is blocked.
pub async fn check_url(url: &reqwest::Url, policy: PrivateNetworkPolicy) -> Result<(), String> {
    let scheme = url.scheme();
    if scheme != "http" && scheme != "https" {
        return Err(format!(
            "blocked URL scheme `{scheme}` — only http and https may be proxied"
        ));
    }
    if !policy.denies_private() {
        return Ok(());
    }

    let host = url
        .host_str()
        .ok_or_else(|| format!("URL `{url}` has no host component"))?;
    // `host_str` brackets IPv6 literals; strip them before parsing.
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    if let Ok(ip) = bare.parse::<IpAddr>() {
        if is_blocked_ip(ip) {
            return Err(format!(
                "destination {ip} is a loopback/private/reserved address; \
                 set {ALLOW_PRIVATE_ENV}=1 to allow it"
            ));
        }
        return Ok(());
    }

    let port = url.port_or_known_default().unwrap_or(80_u16);
    let resolved = tokio::net::lookup_host((host, port))
        .await
        .map_err(|e| format!("DNS resolution failed for `{host}`: {e}"))?;
    let mut saw_any = false;
    for addr in resolved {
        saw_any = true;
        check_socket_addr(addr, policy)?;
    }
    if saw_any {
        Ok(())
    } else {
        Err(format!("DNS resolution returned no addresses for `{host}`"))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn v4(s: &str) -> IpAddr {
        IpAddr::V4(s.parse::<Ipv4Addr>().unwrap())
    }

    fn v6(s: &str) -> IpAddr {
        IpAddr::V6(s.parse::<Ipv6Addr>().unwrap())
    }

    #[test]
    fn blocks_the_classic_ssrf_targets() {
        for raw in [
            "127.0.0.1",
            "127.1.2.3",
            "0.0.0.0",
            "10.1.2.3",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.169.254", // cloud metadata
            "100.64.0.1",      // CGNAT
            "224.0.0.1",       // multicast
            "255.255.255.255",
            "240.0.0.1",
        ] {
            assert!(is_blocked_ip(v4(raw)), "{raw} must be blocked");
        }
    }

    #[test]
    fn blocks_v6_loopback_and_local_ranges() {
        for raw in ["::1", "::", "fe80::1", "fc00::1", "fd12::3", "ff02::1"] {
            assert!(is_blocked_ip(v6(raw)), "{raw} must be blocked");
        }
    }

    #[test]
    fn blocks_v4_mapped_v6_loopback() {
        assert!(
            is_blocked_ip(v6("::ffff:127.0.0.1")),
            "v4-mapped loopback must not slip through the v6 path"
        );
    }

    #[test]
    fn allows_public_addresses() {
        for raw in ["1.1.1.1", "8.8.8.8", "93.184.216.34"] {
            assert!(!is_blocked_ip(v4(raw)), "{raw} must be allowed");
        }
        assert!(!is_blocked_ip(v6("2606:4700:4700::1111")));
    }

    #[test]
    fn allow_policy_skips_the_filter() {
        let addr: SocketAddr = "127.0.0.1:9100".parse().unwrap();
        assert!(check_socket_addr(addr, PrivateNetworkPolicy::Deny).is_err());
        assert!(check_socket_addr(addr, PrivateNetworkPolicy::Allow).is_ok());
    }

    #[tokio::test]
    async fn check_url_rejects_non_http_schemes() {
        for raw in [
            "file:///etc/passwd",
            "ftp://example.com/x",
            "gopher://example.com:70/",
        ] {
            let url = reqwest::Url::parse(raw).unwrap();
            let err = check_url(&url, PrivateNetworkPolicy::Allow)
                .await
                .expect_err("non-http scheme must be rejected even under Allow");
            assert!(err.contains("blocked URL scheme"), "{err}");
        }
    }

    #[tokio::test]
    async fn check_url_rejects_loopback_and_metadata_literals() {
        for raw in [
            "http://127.0.0.1:3000/host/kv/get",
            "http://[::1]:9333/host/exec",
            "http://169.254.169.254/latest/meta-data/",
            "http://192.168.1.1/admin",
        ] {
            let url = reqwest::Url::parse(raw).unwrap();
            assert!(
                check_url(&url, PrivateNetworkPolicy::Deny).await.is_err(),
                "{raw} must be rejected"
            );
            assert!(
                check_url(&url, PrivateNetworkPolicy::Allow).await.is_ok(),
                "{raw} must pass under the dev escape hatch"
            );
        }
    }

    #[test]
    fn policy_defaults_to_deny() {
        assert_eq!(PrivateNetworkPolicy::default(), PrivateNetworkPolicy::Deny);
        assert!(PrivateNetworkPolicy::default().denies_private());
    }
}
