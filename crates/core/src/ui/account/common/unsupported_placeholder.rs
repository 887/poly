//! WP-9 — Defensive placeholder for feature-unsupported views.
//!
//! The capability-based route guards in `routes.rs` redirect users away from
//! views their active backend doesn't support. On the single frame *before*
//! `use_effect` fires the navigator redirect, the view would otherwise render
//! an empty `rsx! {}` and the user would see a blank panel for a brief flicker.
//!
//! This component provides a friendlier mid-flight placeholder so that flicker
//! carries a human-readable explanation ("{backend} doesn't support {feature}")
//! instead of being invisible. It's cheap (one div) and shares styling with the
//! rest of the `special-page-*` shell.

use dioxus::prelude::*;

use crate::i18n::{t, t_args};
use poly_ui_macros::{context_menu, ui_action};

/// Categories of features that can be declared unsupported.
///
/// Each variant maps to a capability on `BackendCapabilities` and provides
/// the FTL key for the message displayed to the user.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnsupportedFeature {
    Friends,
    Dms,
    Notifications,
    CreateServer,
    Voice,
}

impl UnsupportedFeature {
    pub fn label_key(self) -> &'static str {
        match self {
            Self::Friends => "feature-unsupported-friends",
            Self::Dms => "feature-unsupported-dms",
            Self::Notifications => "feature-unsupported-notifications",
            Self::CreateServer => "feature-unsupported-create-server",
            Self::Voice => "feature-unsupported-voice",
        }
    }

    pub fn test_id(self) -> &'static str {
        match self {
            Self::Friends => "feature-unsupported-friends",
            Self::Dms => "feature-unsupported-dms",
            Self::Notifications => "feature-unsupported-notifications",
            Self::CreateServer => "feature-unsupported-create-server",
            Self::Voice => "feature-unsupported-voice",
        }
    }
}

#[ui_action(None)]
#[context_menu(None)]
#[rustfmt::skip]
#[component]
pub fn FeatureUnsupportedPlaceholder(
    backend_slug: String,
    feature: UnsupportedFeature,
) -> Element {
    let backend_display = backend_slug.clone();
    let message = t_args(feature.label_key(), &[("backend", &backend_display)]);
    let redirecting = t("feature-unsupported-redirecting");
    let test_id = feature.test_id();

    rsx! {
        div {
            class: "special-page-content feature-unsupported",
            "data-testid": "{test_id}",
            div { class: "feature-unsupported-inner",
                p { class: "feature-unsupported-message", "{message}" }
                p { class: "feature-unsupported-hint", "{redirecting}" }
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn every_feature_has_a_distinct_label_key() {
        let features = [
            UnsupportedFeature::Friends,
            UnsupportedFeature::Dms,
            UnsupportedFeature::Notifications,
            UnsupportedFeature::CreateServer,
            UnsupportedFeature::Voice,
        ];
        let keys: std::collections::HashSet<&str> =
            features.iter().map(|f| f.label_key()).collect();
        assert_eq!(keys.len(), features.len(), "label keys must be unique");
    }

    #[test]
    fn test_ids_match_label_keys() {
        for feature in [
            UnsupportedFeature::Friends,
            UnsupportedFeature::Dms,
            UnsupportedFeature::Notifications,
            UnsupportedFeature::CreateServer,
            UnsupportedFeature::Voice,
        ] {
            assert_eq!(feature.test_id(), feature.label_key());
        }
    }
}
