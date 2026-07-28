//! # `exec_policy` — program allowlist + user consent for `/host/exec`
//!
//! Phase B of `docs/plans/plan-host-substrate-capability-gating.md`.
//!
//! ## Why
//!
//! `/host/exec` hands a caller `Command::new(program).args(args)`. Before
//! this module there was no allowlist, no path canonicalisation and no
//! consent step, so "reach the loopback port" and "run anything as the
//! user" were the same capability. Phase A closed the *reachability* half
//! by requiring a bearer token; this module closes the *authority* half:
//! even a caller with a valid identity may only run programs that
//!
//! 1. that specific caller **declared** (a plugin's manifest, the shell's
//!    own configuration), and
//! 2. the **user has consented to** at least once for that
//!    `(caller, program)` pair.
//!
//! Default is deny. There is no fallthrough branch.
//!
//! ## Path handling
//!
//! A request names the program in one of exactly two ways:
//!
//! * **An absolute path** with no `..` component. It is resolved with
//!   [`std::fs::canonicalize`] (which follows symlinks) and must equal the
//!   canonicalisation of one of the caller's declared paths. A declared
//!   `/usr/bin/tool` later re-pointed at `/bin/sh` therefore stops
//!   matching unless `/bin/sh` was *also* declared.
//! * **A bare file name** such as `gh`, with no path separator at all.
//!   The name is matched against the *file names of the declared entries*
//!   and resolves to that entry's canonical path. `PATH` is never
//!   consulted, so a caller-controlled environment cannot steer the
//!   lookup — the absolute path always comes from the declaration.
//!
//! Anything else — `./tool`, `bin/tool`, any path containing `..`, an
//! empty string — is rejected before the filesystem is touched.
//!
//! ## Seams (SOLID item 7)
//!
//! [`ExecPolicy`] is the declaration + consent boundary and
//! [`ConsentPrompt`] is the user-notification boundary. Both ship with an
//! in-memory implementation so the gate is testable without a shell, a
//! database or a UI.

use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;

use thiserror::Error;

use crate::host_auth::CallerId;

/// Why an exec request was refused.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum ExecDenied {
    /// The request named a relative path (anything with a separator that
    /// is not absolute), or an empty program.
    #[error("exec denied: `{0}` is neither an absolute path nor a bare program name")]
    NotAbsolute(String),
    /// The request contained a `..` component.
    #[error("exec denied: `{0}` contains a `..` component")]
    Traversal(String),
    /// The path does not resolve to a regular file on disk.
    #[error("exec denied: `{program}` could not be resolved: {reason}")]
    Unresolvable {
        /// The path as requested.
        program: String,
        /// Filesystem-level reason.
        reason: String,
    },
    /// Resolved fine, but this caller never declared it.
    #[error("exec denied: `{program}` is not in the declared program list for {caller}")]
    NotAllowlisted {
        /// [`CallerId::label`] of the caller.
        caller: String,
        /// The resolved path.
        program: String,
    },
    /// Declared, but the user has not approved this pair yet.
    #[error("exec denied: {caller} has not been granted user consent to run `{program}`")]
    NoConsent {
        /// [`CallerId::label`] of the caller.
        caller: String,
        /// The resolved path.
        program: String,
    },
}

/// Source of truth for "what may this caller run" and "has the user said
/// yes".
///
/// Kept deliberately narrow (ISP): it answers two questions and records
/// one decision. Notifying the user is [`ConsentPrompt`]'s job, and
/// resolving/validating paths is [`check_exec`]'s.
pub trait ExecPolicy: Send + Sync {
    /// Absolute paths this caller declared. Empty means "declared
    /// nothing", which denies everything.
    fn declared_programs(&self, caller: &CallerId) -> Vec<PathBuf>;

    /// Replace `caller`'s declared program list.
    ///
    /// Declaring is not consenting — it records what a caller is permitted
    /// to *ask* for, which is the manifest's job; the user still approves
    /// each pair separately.
    ///
    /// # Errors
    ///
    /// Implementation-defined persistence failure.
    fn declare_for(&self, caller: &CallerId, programs: &[PathBuf]) -> Result<(), String>;

    /// Has the user already approved `caller` running `program`?
    /// `program` is always the canonicalised path.
    fn has_consent(&self, caller: &CallerId, program: &Path) -> bool;

    /// Record the user's approval for `(caller, program)`.
    ///
    /// # Errors
    ///
    /// Implementation-defined persistence failure.
    fn grant_consent(&self, caller: &CallerId, program: &Path) -> Result<(), String>;
}

/// Where a "this caller wants to run X, approve?" prompt is surfaced.
///
/// Separate from [`ExecPolicy`] so a headless daemon can log while a UI
/// shell raises a real dialog, without either one implementing the
/// other's methods.
pub trait ConsentPrompt: Send + Sync {
    /// Surface a consent request. Must not block and must not grant —
    /// granting goes through [`ExecPolicy::grant_consent`] once the user
    /// answers.
    fn prompt(&self, caller: &CallerId, program: &Path);
}

/// Emits a `warn` line naming the caller and program. The standalone
/// `poly-host` daemon has no UI, so this is its complete behaviour: the
/// request is denied and the operator sees exactly what to approve.
#[derive(Debug, Default, Clone, Copy)]
pub struct TracingConsentPrompt;

impl ConsentPrompt for TracingConsentPrompt {
    fn prompt(&self, caller: &CallerId, program: &Path) {
        tracing::warn!(
            caller = %caller.label(),
            program = %program.display(),
            "exec consent required — denied until the user approves this (caller, program) pair"
        );
    }
}

/// Records every prompt instead of showing it. Test seam for
/// [`ConsentPrompt`].
#[derive(Debug, Default)]
pub struct RecordingConsentPrompt {
    seen: Mutex<Vec<(String, PathBuf)>>,
}

impl RecordingConsentPrompt {
    /// Prompts surfaced so far, as `(caller label, program)`.
    #[must_use]
    pub fn seen(&self) -> Vec<(String, PathBuf)> {
        self.seen.lock().map(|g| g.clone()).unwrap_or_default()
    }
}

impl ConsentPrompt for RecordingConsentPrompt {
    fn prompt(&self, caller: &CallerId, program: &Path) {
        if let Ok(mut seen) = self.seen.lock() {
            seen.push((caller.label(), program.to_path_buf()));
        }
    }
}

/// Declares nothing for anybody. The default policy for a shell that has
/// not wired a real one — every exec request is refused.
#[derive(Debug, Default, Clone, Copy)]
pub struct DenyAllExecPolicy;

impl ExecPolicy for DenyAllExecPolicy {
    fn declared_programs(&self, _caller: &CallerId) -> Vec<PathBuf> {
        Vec::new()
    }
    fn declare_for(&self, _caller: &CallerId, _programs: &[PathBuf]) -> Result<(), String> {
        Err("DenyAllExecPolicy cannot record declarations".to_string())
    }
    fn has_consent(&self, _caller: &CallerId, _program: &Path) -> bool {
        false
    }
    fn grant_consent(&self, _caller: &CallerId, _program: &Path) -> Result<(), String> {
        Err("DenyAllExecPolicy cannot record consent".to_string())
    }
}

/// In-memory [`ExecPolicy`] — the test seam required by SOLID item 7.
#[derive(Debug, Default)]
pub struct InMemoryExecPolicy {
    declared: Mutex<HashMap<CallerId, Vec<PathBuf>>>,
    consent: Mutex<HashSet<(CallerId, PathBuf)>>,
}

impl InMemoryExecPolicy {
    /// Empty policy: nothing declared, nothing consented.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Declare `programs` for `caller`, replacing any previous list.
    ///
    /// Infallible convenience wrapper over
    /// [`ExecPolicy::declare_for`] for test setup.
    pub fn declare(&self, caller: &CallerId, programs: Vec<PathBuf>) {
        if let Ok(mut map) = self.declared.lock() {
            let _prev = map.insert(caller.clone(), programs);
        }
    }
}

impl ExecPolicy for InMemoryExecPolicy {
    fn declared_programs(&self, caller: &CallerId) -> Vec<PathBuf> {
        self.declared
            .lock()
            .ok()
            .and_then(|m| m.get(caller).cloned())
            .unwrap_or_default()
    }

    fn declare_for(&self, caller: &CallerId, programs: &[PathBuf]) -> Result<(), String> {
        let mut map = self
            .declared
            .lock()
            .map_err(|_poison| "declaration map poisoned".to_string())?;
        let _prev = map.insert(caller.clone(), programs.to_vec());
        Ok(())
    }

    fn has_consent(&self, caller: &CallerId, program: &Path) -> bool {
        self.consent
            .lock()
            .is_ok_and(|c| c.contains(&(caller.clone(), program.to_path_buf())))
    }

    fn grant_consent(&self, caller: &CallerId, program: &Path) -> Result<(), String> {
        let mut set = self
            .consent
            .lock()
            .map_err(|_poison| "consent set poisoned".to_string())?;
        let _added = set.insert((caller.clone(), program.to_path_buf()));
        Ok(())
    }
}

/// Resolve `program` and decide whether `caller` may run it.
///
/// Returns the canonicalised path to hand to `Command::new`. Callers must
/// spawn *this* path, not the requested string, so the check and the spawn
/// cannot disagree.
///
/// # Errors
///
/// [`ExecDenied`] — default deny; there is no success branch that skips
/// both the allowlist and the consent check.
pub fn check_exec(
    policy: &dyn ExecPolicy,
    caller: &CallerId,
    program: &str,
) -> Result<PathBuf, ExecDenied> {
    let trimmed = program.trim();
    let declared: Vec<PathBuf> = policy
        .declared_programs(caller)
        .iter()
        .filter_map(|p| std::fs::canonicalize(p).ok())
        .collect();

    let label = caller.label();
    let resolved = match selector(trimmed)? {
        Selector::Absolute(path) => match_absolute(&declared, &path, trimmed, &label)?,
        Selector::Name(name) => match_by_name(&declared, name, trimmed, &label)?,
    };
    ensure_regular_file(&resolved, trimmed)?;

    if !policy.has_consent(caller, &resolved) {
        return Err(ExecDenied::NoConsent {
            caller: caller.label(),
            program: resolved.display().to_string(),
        });
    }
    Ok(resolved)
}

/// The two — and only two — shapes a request may take.
enum Selector<'a> {
    /// An absolute, `..`-free path.
    Absolute(PathBuf),
    /// A bare file name with no separator.
    Name(&'a str),
}

fn selector(trimmed: &str) -> Result<Selector<'_>, ExecDenied> {
    if trimmed.is_empty() {
        return Err(ExecDenied::NotAbsolute(String::new()));
    }
    let path = Path::new(trimmed);
    if path.components().any(|c| c == Component::ParentDir) {
        return Err(ExecDenied::Traversal(trimmed.to_string()));
    }
    if path.is_absolute() {
        return Ok(Selector::Absolute(path.to_path_buf()));
    }
    // A single normal component and nothing else = a bare program name.
    let mut components = path.components();
    match (components.next(), components.next()) {
        (Some(Component::Normal(_)), None) => Ok(Selector::Name(trimmed)),
        _other => Err(ExecDenied::NotAbsolute(trimmed.to_string())),
    }
}

fn match_absolute(
    declared: &[PathBuf],
    path: &Path,
    requested: &str,
    caller: &str,
) -> Result<PathBuf, ExecDenied> {
    let resolved = std::fs::canonicalize(path).map_err(|e| ExecDenied::Unresolvable {
        program: requested.to_string(),
        reason: e.to_string(),
    })?;
    if declared.contains(&resolved) {
        Ok(resolved)
    } else {
        Err(ExecDenied::NotAllowlisted {
            caller: caller.to_string(),
            program: resolved.display().to_string(),
        })
    }
}

fn match_by_name(
    declared: &[PathBuf],
    name: &str,
    requested: &str,
    caller: &str,
) -> Result<PathBuf, ExecDenied> {
    declared
        .iter()
        .find(|d| d.file_name().is_some_and(|f| f == name))
        .cloned()
        .ok_or_else(|| ExecDenied::NotAllowlisted {
            caller: caller.to_string(),
            program: requested.to_string(),
        })
}

fn ensure_regular_file(resolved: &Path, requested: &str) -> Result<(), ExecDenied> {
    let meta = std::fs::metadata(resolved).map_err(|e| ExecDenied::Unresolvable {
        program: requested.to_string(),
        reason: e.to_string(),
    })?;
    if meta.is_file() {
        Ok(())
    } else {
        Err(ExecDenied::Unresolvable {
            program: requested.to_string(),
            reason: "not a regular file".to_string(),
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn plugin(id: &str) -> CallerId {
        CallerId::Plugin { id: id.to_string() }
    }

    /// A real, canonicalisable executable that exists on every CI box.
    fn probe_program() -> PathBuf {
        for candidate in ["/bin/echo", "/usr/bin/echo", "/bin/sh"] {
            let p = Path::new(candidate);
            if p.exists() {
                return p.to_path_buf();
            }
        }
        panic!("no probe program found");
    }

    #[test]
    fn deny_all_policy_denies_everything() {
        let prog = probe_program();
        let err = check_exec(
            &DenyAllExecPolicy,
            &CallerId::Shell,
            &prog.display().to_string(),
        )
        .unwrap_err();
        assert!(matches!(err, ExecDenied::NotAllowlisted { .. }), "{err:?}");
    }

    #[test]
    fn undeclared_program_is_denied() {
        let policy = InMemoryExecPolicy::new();
        policy.declare(&plugin("a"), vec![probe_program()]);
        let err = check_exec(&policy, &plugin("a"), "/usr/bin/definitely-not-declared")
            .unwrap_err();
        // Either it doesn't exist on this box, or it exists and isn't declared —
        // both are a denial, never an execution.
        assert!(
            matches!(
                err,
                ExecDenied::Unresolvable { .. } | ExecDenied::NotAllowlisted { .. }
            ),
            "{err:?}"
        );
    }

    #[test]
    fn declared_program_still_needs_consent() {
        let policy = InMemoryExecPolicy::new();
        let prog = probe_program();
        policy.declare(&plugin("a"), vec![prog.clone()]);

        let err = check_exec(&policy, &plugin("a"), &prog.display().to_string()).unwrap_err();
        assert!(matches!(err, ExecDenied::NoConsent { .. }), "{err:?}");

        let canonical = std::fs::canonicalize(&prog).unwrap();
        policy.grant_consent(&plugin("a"), &canonical).unwrap();
        let ok = check_exec(&policy, &plugin("a"), &prog.display().to_string()).unwrap();
        assert_eq!(ok, canonical);
    }

    #[test]
    fn one_plugins_declaration_does_not_cover_another() {
        let policy = InMemoryExecPolicy::new();
        let prog = probe_program();
        let canonical = std::fs::canonicalize(&prog).unwrap();
        policy.declare(&plugin("a"), vec![prog.clone()]);
        policy.grant_consent(&plugin("a"), &canonical).unwrap();

        let err = check_exec(&policy, &plugin("b"), &prog.display().to_string()).unwrap_err();
        assert!(matches!(err, ExecDenied::NotAllowlisted { .. }), "{err:?}");
    }

    #[test]
    fn relative_paths_are_denied() {
        let policy = InMemoryExecPolicy::new();
        policy.declare(&CallerId::Shell, vec![probe_program()]);
        for bad in ["./echo", "bin/echo", "sub/dir/echo", ""] {
            let err = check_exec(&policy, &CallerId::Shell, bad).unwrap_err();
            assert!(matches!(err, ExecDenied::NotAbsolute(_)), "{bad}: {err:?}");
        }
    }

    /// A bare name is a *selector into the declaration list*, never a PATH
    /// lookup: it only resolves if the caller declared an entry with that
    /// file name, and it resolves to that entry's absolute path.
    #[test]
    fn bare_name_resolves_through_the_declaration_not_path() {
        let policy = InMemoryExecPolicy::new();
        let prog = probe_program();
        let canonical = std::fs::canonicalize(&prog).unwrap();
        let name = canonical.file_name().unwrap().to_string_lossy().to_string();

        // Undeclared bare name: denied even though it is on PATH.
        let err = check_exec(&policy, &CallerId::Shell, &name).unwrap_err();
        assert!(matches!(err, ExecDenied::NotAllowlisted { .. }), "{err:?}");

        policy.declare(&CallerId::Shell, vec![prog]);
        let err = check_exec(&policy, &CallerId::Shell, &name).unwrap_err();
        assert!(matches!(err, ExecDenied::NoConsent { .. }), "{err:?}");

        policy.grant_consent(&CallerId::Shell, &canonical).unwrap();
        assert_eq!(
            check_exec(&policy, &CallerId::Shell, &name).unwrap(),
            canonical
        );
    }

    #[test]
    fn parent_dir_traversal_is_denied_before_the_filesystem_is_touched() {
        let policy = InMemoryExecPolicy::new();
        let prog = probe_program();
        let canonical = std::fs::canonicalize(&prog).unwrap();
        policy.declare(&CallerId::Shell, vec![prog.clone()]);
        policy.grant_consent(&CallerId::Shell, &canonical).unwrap();

        // Traversal that would canonicalise back onto the allowlisted target.
        let traversal = format!("/usr/../{}", prog.display().to_string().trim_start_matches('/'));
        let err = check_exec(&policy, &CallerId::Shell, &traversal).unwrap_err();
        assert!(matches!(err, ExecDenied::Traversal(_)), "{err:?}");
    }

    #[test]
    fn directories_are_not_executable_targets() {
        let policy = InMemoryExecPolicy::new();
        policy.declare(&CallerId::Shell, vec![PathBuf::from("/usr")]);
        let err = check_exec(&policy, &CallerId::Shell, "/usr").unwrap_err();
        assert!(matches!(err, ExecDenied::Unresolvable { .. }), "{err:?}");
    }

    #[test]
    fn recording_prompt_captures_the_pair() {
        let prompt = RecordingConsentPrompt::default();
        prompt.prompt(&plugin("a"), Path::new("/bin/echo"));
        assert_eq!(
            prompt.seen(),
            vec![("plugin:a".to_string(), PathBuf::from("/bin/echo"))]
        );
    }
}
