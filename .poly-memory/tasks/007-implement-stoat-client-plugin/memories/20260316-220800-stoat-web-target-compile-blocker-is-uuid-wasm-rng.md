# Memory: Stoat web-target compile blocker is uuid wasm RNG

*Stored: 2026-03-16T22:08:00.926724516+00:00*

---

Focused feasibility check on 2026-03-16:

Command:
`cargo check -p poly-stoat --target wasm32-unknown-unknown`

Result:
- The Stoat crate does begin compiling for `wasm32-unknown-unknown`.
- The first concrete blocker is **not** reqwest/tokio yet; it fails in `uuid 1.21.0` because no wasm randomness source is enabled.
- Error: `to use uuid on wasm32-unknown-unknown, specify a source of randomness using one of the js, rng-getrandom, or rng-rand features`.

Implication:
- Compiling Stoat directly into `poly-web` looks plausible.
- The immediate Cargo-side fix is to enable a wasm-compatible uuid RNG feature (likely `js` or `rng-getrandom`) for `poly-stoat` / workspace usage before wiring Stoat into the web build.
