//! Shared avatar-serving helper for test servers.
//!
//! Maps animal names to bundled image bytes from `clients/demo/assets/`.
//! Each backend's avatar route becomes a thin wrapper around `serve_animal`.
//!
//! Supported animals:
//!   PNG: koala, kangaroo, platypus, owl, raccoon, stoat, lemming, sheep,
//!        walrus, cat, dog, parrot, cockatoo
//!   SVG: axolotl, beaver, hedgehog, flamingo, otter, owl, raccoon, parrot,
//!        cockatoo, sheep, walrus, cat, dog

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

/// Serve a bundled animal image by name.
///
/// `name` is the bare animal name without extension (e.g. `"sheep"`).
/// Returns PNG for animals that have a PNG asset, SVG for SVG-only animals.
/// Returns 404 for unknown names.
#[must_use]
pub fn serve_animal(name: &str) -> Response {
    // PNG assets
    static KOALA_PNG: &[u8] = include_bytes!("../../../clients/demo/assets/koala.png");
    static KANGAROO_PNG: &[u8] = include_bytes!("../../../clients/demo/assets/kangaroo.png");
    static PLATYPUS_PNG: &[u8] = include_bytes!("../../../clients/demo/assets/platypus.png");
    static OWL_PNG: &[u8] = include_bytes!("../../../clients/demo/assets/owl.png");
    static RACCOON_PNG: &[u8] = include_bytes!("../../../clients/demo/assets/raccoon.png");
    static STOAT_PNG: &[u8] = include_bytes!("../../../clients/demo/assets/stoat.png");
    static LEMMING_PNG: &[u8] = include_bytes!("../../../clients/demo/assets/lemming.png");
    static SHEEP_PNG: &[u8] = include_bytes!("../../../clients/demo/assets/sheep.png");
    static WALRUS_PNG: &[u8] = include_bytes!("../../../clients/demo/assets/walrus.png");
    static CAT_PNG: &[u8] = include_bytes!("../../../clients/demo/assets/cat.png");
    static DOG_PNG: &[u8] = include_bytes!("../../../clients/demo/assets/dog.png");
    static PARROT_PNG: &[u8] = include_bytes!("../../../clients/demo/assets/parrot.png");
    static COCKATOO_PNG: &[u8] = include_bytes!("../../../clients/demo/assets/cockatoo.png");

    // SVG assets
    static AXOLOTL_SVG: &[u8] = include_bytes!("../../../clients/demo/assets/axolotl.svg");
    static BEAVER_SVG: &[u8] = include_bytes!("../../../clients/demo/assets/beaver.svg");
    static HEDGEHOG_SVG: &[u8] = include_bytes!("../../../clients/demo/assets/hedgehog.svg");
    static FLAMINGO_SVG: &[u8] = include_bytes!("../../../clients/demo/assets/flamingo.svg");
    static OTTER_SVG: &[u8] = include_bytes!("../../../clients/demo/assets/otter.svg");

    let (bytes, mime): (&[u8], &str) = match name {
        "koala" => (KOALA_PNG, "image/png"),
        "kangaroo" => (KANGAROO_PNG, "image/png"),
        "platypus" => (PLATYPUS_PNG, "image/png"),
        "owl" => (OWL_PNG, "image/png"),
        "raccoon" => (RACCOON_PNG, "image/png"),
        "stoat" => (STOAT_PNG, "image/png"),
        "lemming" => (LEMMING_PNG, "image/png"),
        "sheep" => (SHEEP_PNG, "image/png"),
        "walrus" => (WALRUS_PNG, "image/png"),
        "cat" => (CAT_PNG, "image/png"),
        "dog" => (DOG_PNG, "image/png"),
        "parrot" => (PARROT_PNG, "image/png"),
        "cockatoo" => (COCKATOO_PNG, "image/png"),
        "axolotl" => (AXOLOTL_SVG, "image/svg+xml"),
        "beaver" => (BEAVER_SVG, "image/svg+xml"),
        "hedgehog" => (HEDGEHOG_SVG, "image/svg+xml"),
        "flamingo" => (FLAMINGO_SVG, "image/svg+xml"),
        "otter" => (OTTER_SVG, "image/svg+xml"),
        _ => return (StatusCode::NOT_FOUND, "unknown avatar").into_response(),
    };
    (
        StatusCode::OK,
        [
            (axum::http::header::CONTENT_TYPE, mime),
            (axum::http::header::CACHE_CONTROL, "public, max-age=3600"),
        ],
        bytes,
    )
        .into_response()
}
