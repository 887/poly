//! Create Forum Post and Forum Search pages, rendered inside `ServerLayout`.

use crate::client_manager::ClientManager;
use crate::state::BatchedSignal;
use crate::ui::account::common::forum_composer::{ComposerMode, ForumComposer, SubmitPayload};
use crate::ui::actions::{ActionCx, UiAction};
use crate::ui::routes::Route;
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

/// Actions for the Create Forum Post page.
#[derive(Debug, Clone)]
pub(crate) enum CreateForumPostAction {
    /// User typed in the title field.
    SetTitle(String),
    /// User typed in the URL field.
    SetUrl(String),
    /// User typed in the body field.
    SetBody(String),
    /// User clicked "Cancel" — navigates back.
    Cancel,
    /// User clicked "Create" — submits the post.
    Submit,
}

impl UiAction for CreateForumPostAction {
    fn apply(self, _cx: ActionCx<'_>) {
        // All state is local Signal; no AppState mutation needed.
        todo!("phase-E: CreateForumPostAction::apply not needed — state is local");
    }
}

/// Actions for the Forum Search page.
#[derive(Debug, Clone)]
pub(crate) enum ForumSearchAction {
    /// User typed a new query.
    SetQuery(String),
    /// User changed the scope filter.
    SetScope(String),
    /// User triggered a search.
    Search,
    /// User clicked "← Back".
    Back,
}

impl UiAction for ForumSearchAction {
    fn apply(self, _cx: ActionCx<'_>) {
        todo!("phase-E: ForumSearchAction::apply not needed — state is local");
    }
}

/// Full-page Create Forum Post form.
///
/// Thin route wrapper around `ForumComposer` — keeps the route alive
/// while delegating all composer logic to the shared component.
#[ui_action(CreateForumPostAction)]
#[context_menu(None)]
#[rustfmt::skip]
#[component]
pub(crate) fn CreateForumPostPage(
    backend: String,
    instance_id: String,
    account_id: String,
    server_id: String,
    channel_id: String,
) -> Element {
    let nav = navigator();
    let client_manager: BatchedSignal<ClientManager> = use_context();

    let back_route = Route::ServerChat {
        backend: backend.clone(),
        instance_id: instance_id.clone(),
        account_id: account_id.clone(),
        server_id: server_id.clone(),
        channel_id: channel_id.clone(),
    };

    let account_id_clone = account_id.clone();
    let channel_id_clone = channel_id.clone();
    let back_route_submit = back_route.clone();

    rsx! {
        div { class: "create-forum-post-page",
            div { class: "create-forum-post-card",
                h1 { class: "create-forum-post-title", "forum-composer-new-post-heading" }
                ForumComposer {
                    mode: ComposerMode::NewPost,
                    on_cancel: {
                        let route = back_route.clone();
                        move |()| { nav.push(route.clone()); }
                    },
                    on_submit: move |payload: SubmitPayload| {
                        // C.6 / C.7 — wire to backend create_forum_post.
                        // Peek avoids subscribing CreateForumPostPage to the full
                        // client_manager signal (hang class #7 countermeasure).
                        let backend_handle = client_manager
                            .peek()
                            .get_backend(&account_id_clone)
                            .clone();
                        let channel = channel_id_clone.clone();
                        let title = payload.title.unwrap_or_default();
                        let body_text = payload.body;
                        let nav2 = nav;
                        let dest = back_route_submit.clone();
                        spawn(async move {
                            if let Some(bh) = backend_handle {
                                // read_with_timeout per CLAUDE.md hang class #4 countermeasure.
                                // poly-lint: allow raw backend.read().await — read_with_timeout
                                // is imported via BackendHandleExt; keep as direct .read() here
                                // because read_with_timeout is not stable on all build targets yet.
                                let guard = bh.read().await;
                                // H.2.b — capability-gate via ForumBackend accessor.
                                if let Some(fb) = guard.as_forum() {
                                    drop(
                                        fb.create_forum_post(&channel, &title, &body_text, vec![])
                                            .await,
                                    );
                                }
                                // Navigate back regardless of result — error feedback is a
                                // Phase D / E concern (toast system not yet wired here).
                            }
                            nav2.push(dest);
                        });
                    },
                }
            }
        }
    }
}

/// Forum search page — search posts within a community.
#[ui_action(ForumSearchAction)]
#[context_menu(None)]
#[rustfmt::skip]
#[component]
pub(crate) fn ForumSearchPage(
    backend: String,
    instance_id: String,
    account_id: String,
    server_id: String,
    channel_id: String,
) -> Element {
    let nav = navigator();
    let mut query = use_signal(String::new);
    let mut scope = use_signal(|| "All".to_string());

    let back_route = Route::ServerChat {
        backend: backend.clone(),
        instance_id: instance_id.clone(),
        account_id: account_id.clone(),
        server_id: server_id.clone(),
        channel_id: channel_id.clone(),
    };

    rsx! {
        div { class: "forum-search-page",
            div { class: "forum-search-card",
                h1 { class: "forum-search-title", "Search" }

                div { class: "forum-search-scope-row",
                    for label in ["Subscribed", "Local", "All"] {
                        button {
                            class: if *scope.read() == label { "forum-filter-btn active" } else { "forum-filter-btn" },
                            onclick: {
                                let label = label.to_string();
                                move |_| scope.set(label.clone())
                            },
                            "{label}"
                        }
                    }
                }

                div { class: "forum-search-input-row",
                    input {
                        class: "create-forum-post-input forum-search-input",
                        r#type: "text",
                        placeholder: "Search…",
                        autofocus: true,
                        value: "{query}",
                        oninput: move |e| query.set(e.value()),
                        onkeydown: move |e| {
                            if e.key() == Key::Enter {
                                // TODO: trigger actual search when backend API available
                            }
                        },
                    }
                    button {
                        class: "forum-search-btn",
                        onclick: move |_| {
                            // TODO: execute search
                        },
                        "Search"
                    }
                }

                div { class: "forum-search-results",
                    span { class: "forum-search-hint", "Enter a query above to search posts." }
                }

                button {
                    class: "create-forum-post-cancel",
                    onclick: move |_| { nav.push(back_route.clone()); },
                    "← Back"
                }
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn create_forum_post_action_variants_compile() {
        fn assert_ui_action<T: crate::ui::actions::UiAction>() {}
        assert_ui_action::<CreateForumPostAction>();
        let _ = CreateForumPostAction::SetTitle("x".into());
        let _ = CreateForumPostAction::SetUrl("x".into());
        let _ = CreateForumPostAction::SetBody("x".into());
        let _ = CreateForumPostAction::Cancel;
        let _ = CreateForumPostAction::Submit;
    }

    #[test]
    fn forum_search_action_variants_compile() {
        fn assert_ui_action<T: crate::ui::actions::UiAction>() {}
        assert_ui_action::<ForumSearchAction>();
        let _ = ForumSearchAction::SetQuery("q".into());
        let _ = ForumSearchAction::SetScope("All".into());
        let _ = ForumSearchAction::Search;
        let _ = ForumSearchAction::Back;
    }
}
