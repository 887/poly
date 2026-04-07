//! Forum view — Lemmy/Reddit-style post list + threaded comment view
//! for `ChannelType::Forum` channels, and Hacker News feed view for
//! `ChannelType::HackerNews` channels.

use crate::client_manager::ClientManager;
use crate::state::chat_data::{backend_badge, user_color};
use crate::state::{AppState, ChatData};
use crate::ui::favorites_sidebar::restore_server_channel;
use crate::ui::routes::Route;
use chrono::DateTime;
use dioxus::prelude::*;
use poly_client::{ChannelType, Message, MessageContent, MessageQuery};

const PAGE_SIZE: usize = 20;

// ─────────────────────────────────────────────────────────────────────────────
// Sort
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum ForumSort {
    #[default]
    Hot,
    Active,
    Scaled,
    Controversial,
    New,
    Old,
    MostComments,
    NewComments,
    TopHour,
    TopSixHours,
    TopTwelveHours,
    TopDay,
    TopWeek,
    TopMonth,
    TopThreeMonths,
    TopSixMonths,
    TopNineMonths,
    TopYear,
    TopAllTime,
}

impl ForumSort {
    fn value(self) -> &'static str {
        match self {
            Self::Hot => "hot", Self::Active => "active", Self::Scaled => "scaled",
            Self::Controversial => "controversial", Self::New => "new", Self::Old => "old",
            Self::MostComments => "most_comments", Self::NewComments => "new_comments",
            Self::TopHour => "top_hour", Self::TopSixHours => "top_six_hours",
            Self::TopTwelveHours => "top_twelve_hours", Self::TopDay => "top_day",
            Self::TopWeek => "top_week", Self::TopMonth => "top_month",
            Self::TopThreeMonths => "top_three_months", Self::TopSixMonths => "top_six_months",
            Self::TopNineMonths => "top_nine_months", Self::TopYear => "top_year",
            Self::TopAllTime => "top_all_time",
        }
    }

    fn from_value(s: &str) -> Self {
        match s {
            "active" => Self::Active, "scaled" => Self::Scaled,
            "controversial" => Self::Controversial, "new" => Self::New, "old" => Self::Old,
            "most_comments" => Self::MostComments, "new_comments" => Self::NewComments,
            "top_hour" => Self::TopHour, "top_six_hours" => Self::TopSixHours,
            "top_twelve_hours" => Self::TopTwelveHours, "top_day" => Self::TopDay,
            "top_week" => Self::TopWeek, "top_month" => Self::TopMonth,
            "top_three_months" => Self::TopThreeMonths, "top_six_months" => Self::TopSixMonths,
            "top_nine_months" => Self::TopNineMonths, "top_year" => Self::TopYear,
            "top_all_time" => Self::TopAllTime, _ => Self::Hot,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Comment tree node — stores a Message + its recursively resolved children
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Clone, PartialEq)]
struct ForumCommentNode {
    msg: Message,
    depth: u32,
    children: Vec<ForumCommentNode>,
}

fn build_comment_tree(post_id: &str, comments: &[Message]) -> Vec<ForumCommentNode> {
    fn children_of(parent_id: &str, all: &[Message], depth: u32) -> Vec<ForumCommentNode> {
        if depth > 8 {
            return vec![];
        }
        all.iter()
            .filter(|m| {
                m.reply_to
                    .as_ref()
                    .is_some_and(|r| r.message_id == parent_id)
            })
            .map(|m| ForumCommentNode {
                children: children_of(&m.id, all, depth + 1),
                msg: m.clone(),
                depth,
            })
            .collect()
    }
    comments
        .iter()
        .filter(|m| {
            m.reply_to.is_none()
                || m.reply_to
                    .as_ref()
                    .is_some_and(|r| r.message_id == post_id)
        })
        .map(|m| ForumCommentNode {
            children: children_of(&m.id, comments, 1),
            msg: m.clone(),
            depth: 0,
        })
        .collect()
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn reaction_count(msg: &Message, emoji: &str) -> u32 {
    msg.reactions
        .iter()
        .find(|r| r.emoji == emoji)
        .map(|r| r.count)
        .unwrap_or(0)
}

fn post_score(msg: &Message) -> i64 {
    let up = reaction_count(msg, "🔥")
        .max(reaction_count(msg, "❤️"))
        .max(reaction_count(msg, "👍"))
        .max(reaction_count(msg, "🎉"))
        .max(reaction_count(msg, "🦀")) as i64;
    let down = reaction_count(msg, "👎") as i64;
    up - down
}

fn post_text(content: &MessageContent) -> &str {
    match content {
        MessageContent::Text(s) => s.as_str(),
        MessageContent::WithAttachments { text, .. } => text.as_str(),
    }
}

fn forum_ts(ts: DateTime<chrono::Utc>) -> String {
    let local = ts.with_timezone(&chrono::Local);
    let now = chrono::Local::now();
    let diff = now.signed_duration_since(local);
    let secs = diff.num_seconds();
    if secs < 60 {
        return "just now".to_string();
    }
    let m = diff.num_minutes();
    if m < 60 {
        return format!("{m}m ago");
    }
    let h = diff.num_hours();
    if h < 24 {
        return format!("{h}h ago");
    }
    let d = diff.num_days();
    if d < 7 {
        return format!("{d}d ago");
    }
    local.format("%b %-d, %Y").to_string()
}

fn score_class(score: i64) -> &'static str {
    if score > 0 {
        "forum-score positive"
    } else if score < 0 {
        "forum-score negative"
    } else {
        "forum-score"
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Top-level ForumView — dispatches to HN feed or Lemmy forum based on channel type
// ─────────────────────────────────────────────────────────────────────────────

#[rustfmt::skip]
#[component]
pub fn ForumView() -> Element {
    let chat_data: Signal<ChatData> = use_context();
    let is_hn = chat_data.read().current_channel.as_ref()
        .is_some_and(|ch| ch.channel_type == ChannelType::HackerNews);
    if is_hn { rsx! { HnFeedView {} } } else { rsx! { LemmyForumView {} } }
}

// ─────────────────────────────────────────────────────────────────────────────
// Hacker News feed view — filter input, infinite scroll, no Lemmy sort dropdown
// ─────────────────────────────────────────────────────────────────────────────

#[rustfmt::skip]
#[component]
fn HnFeedView() -> Element {
    let chat_data: Signal<ChatData> = use_context();
    let app_state: Signal<AppState> = use_context();
    let client_manager: Signal<ClientManager> = use_context();
    let nav = navigator();

    let mut filter = use_signal(String::new);
    let mut visible_count = use_signal(|| PAGE_SIZE);

    let channel_id_for_reset = app_state.read().nav.selected_channel.clone().unwrap_or_default();
    use_effect(move || {
        let _ = channel_id_for_reset.clone();
        visible_count.set(PAGE_SIZE);
        filter.set(String::new());
    });

    let snapshot = chat_data.read();
    let current_channel = snapshot.current_channel.clone();
    let current_server = snapshot.current_server.clone();
    let posts = snapshot.messages.clone();
    drop(snapshot);

    let channel_name = current_channel.as_ref().map(|ch| ch.name.clone()).unwrap_or_default();
    let server_name = current_server
        .as_ref()
        .map(|s| format!("{} {}", backend_badge(&s.backend), s.backend.display_name()))
        .unwrap_or_default();

    let route_params = {
        let s = app_state.read();
        (
            s.nav.active_backend.as_ref().map(|b| b.slug().to_string()).unwrap_or_default(),
            s.nav.active_instance_id.clone().unwrap_or_default(),
            s.nav.active_account_id.clone().unwrap_or_default(),
            s.nav.selected_server.clone().unwrap_or_default(),
            s.nav.selected_channel.clone().unwrap_or_default(),
        )
    };

    let filter_text = filter.read().to_lowercase();
    let filtered: Vec<Message> = if filter_text.is_empty() {
        posts
    } else {
        posts.into_iter().filter(|p| post_text(&p.content).to_lowercase().contains(&filter_text)).collect()
    };

    let vc = *visible_count.read();
    let total = filtered.len();
    let has_more = total > vc;
    let visible_posts: Vec<Message> = filtered.into_iter().take(vc).collect();

    rsx! {
        div { class: "forum-view",
            div { class: "forum-header",
                div { class: "forum-header-info",
                    span { class: "forum-channel-name", "🟠 {channel_name}" }
                    if !server_name.is_empty() {
                        span { class: "chat-source-badge", "{server_name}" }
                    }
                }
                div { class: "hn-feed-controls",
                    input {
                        class: "hn-filter-input",
                        r#type: "text",
                        placeholder: "Filter posts…",
                        value: "{filter.read()}",
                        oninput: move |e| {
                            filter.set(e.value());
                            visible_count.set(PAGE_SIZE);
                        },
                    }
                    button {
                        class: "forum-refresh-btn",
                        title: "Refresh",
                        onclick: {
                            let account_id = app_state.read().nav.active_account_id.clone();
                            let b = account_id.as_deref()
                                .and_then(|aid| client_manager.read().get_backend(aid));
                            let channel_id = app_state.read().nav.selected_channel.clone().unwrap_or_default();
                            move |_| {
                                if let Some(ref b) = b {
                                    let b = b.clone();
                                    let cid = channel_id.clone();
                                    let mut cd = chat_data;
                                    spawn(async move {
                                        let msgs = b.read().await
                                            .get_messages(&cid, MessageQuery::default())
                                            .await
                                            .unwrap_or_default();
                                        cd.write().messages = msgs;
                                    });
                                }
                            }
                        },
                        "↻"
                    }
                }
            }
            div {
                class: "hn-feed-list",
                onscroll: move |_| {
                    if has_more {
                        spawn(async move {
                            let near = document::eval(
                                "(function(){var e=document.querySelector('.hn-feed-list');\
                                return e?(e.scrollTop+e.clientHeight>=e.scrollHeight-400):false;})()"
                            )
                            .await
                            .ok()
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                            if near { visible_count.set(vc + PAGE_SIZE); }
                        });
                    }
                },
                if visible_posts.is_empty() {
                    div { class: "forum-empty",
                        div { class: "forum-empty-icon", "🟠" }
                        p { "No posts." }
                    }
                }
                for post in visible_posts {
                    {
                        let post2 = post.clone();
                        let post_id = post.id.clone();
                        let (backend, instance_id, account_id2, server_id, channel_id) = route_params.clone();
                        let nav2 = nav.clone();
                        rsx! {
                            ForumPostCard {
                                key: "{post_id}",
                                post: post2.clone(),
                                on_click: move |_| {
                                    nav2.push(Route::ForumPostRoute {
                                        backend: backend.clone(),
                                        instance_id: instance_id.clone(),
                                        account_id: account_id2.clone(),
                                        server_id: server_id.clone(),
                                        channel_id: channel_id.clone(),
                                        post_id: post_id.clone(),
                                    });
                                },
                            }
                        }
                    }
                }
                if has_more {
                    div { class: "forum-load-more hn-load-more", id: "forum-scroll-sentinel",
                        onclick: move |_| visible_count.set(vc + PAGE_SIZE),
                        "Loading more…"
                    }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Lemmy/Reddit-style forum view (original implementation)
// ─────────────────────────────────────────────────────────────────────────────

#[rustfmt::skip]
#[component]
fn LemmyForumView() -> Element {
    let chat_data: Signal<ChatData> = use_context();
    let app_state: Signal<AppState> = use_context();
    let client_manager: Signal<ClientManager> = use_context();
    let nav = navigator();

    let mut sort = use_signal(|| ForumSort::Hot);
    let mut visible_count = use_signal(|| PAGE_SIZE);

    // Reset visible count when channel changes so scroll-position doesn't leak between channels.
    let channel_id_for_reset = app_state.read().nav.selected_channel.clone().unwrap_or_default();
    use_effect(move || {
        let _ = channel_id_for_reset.clone(); // track dependency
        visible_count.set(PAGE_SIZE);
    });

    let snapshot = chat_data.read();
    let current_channel = snapshot.current_channel.clone();
    let current_server = snapshot.current_server.clone();
    let posts = snapshot.messages.clone();
    drop(snapshot);

    let channel_name = current_channel.as_ref().map(|ch| ch.name.clone()).unwrap_or_default();
    let server_name = current_server
        .as_ref()
        .map(|s| format!("{} {}", backend_badge(&s.backend), s.backend.display_name()))
        .unwrap_or_default();

    // Route params for post navigation
    let route_params = {
        let s = app_state.read();
        (
            s.nav.active_backend.as_ref().map(|b| b.slug().to_string()).unwrap_or_default(),
            s.nav.active_instance_id.clone().unwrap_or_default(),
            s.nav.active_account_id.clone().unwrap_or_default(),
            s.nav.selected_server.clone().unwrap_or_default(),
            s.nav.selected_channel.clone().unwrap_or_default(),
        )
    };

    let account_id = app_state.read().nav.active_account_id.clone();
    let _backend_for_load = account_id.as_deref()
        .and_then(|aid| client_manager.read().get_backend(aid));

    let mut sorted_posts = posts.clone();
    match *sort.read() {
        ForumSort::Hot | ForumSort::Active | ForumSort::Scaled | ForumSort::Controversial
        | ForumSort::MostComments | ForumSort::TopHour | ForumSort::TopSixHours
        | ForumSort::TopTwelveHours | ForumSort::TopDay | ForumSort::TopWeek
        | ForumSort::TopMonth | ForumSort::TopThreeMonths | ForumSort::TopSixMonths
        | ForumSort::TopNineMonths | ForumSort::TopYear | ForumSort::TopAllTime => {
            sorted_posts.sort_by(|a, b| post_score(b).cmp(&post_score(a)))
        }
        ForumSort::New | ForumSort::NewComments => {
            sorted_posts.sort_by(|a, b| b.timestamp.cmp(&a.timestamp))
        }
        ForumSort::Old => sorted_posts.sort_by(|a, b| a.timestamp.cmp(&b.timestamp)),
    }

    let current_sort = *sort.read();
    let vc = *visible_count.read();
    let total_posts = sorted_posts.len();
    let has_more = total_posts > vc;
    // Drain sorted_posts — must be after total_posts is computed.
    let visible_posts: Vec<Message> = sorted_posts.into_iter().take(vc).collect();

    rsx! {
        div { class: "forum-view",
            // Header with sort tabs
            div { class: "forum-header",
                div { class: "forum-header-info",
                    span { class: "forum-channel-name", "📋 {channel_name}" }
                    if !server_name.is_empty() {
                        span { class: "chat-source-badge", "{server_name}" }
                    }
                }
                div { class: "forum-sort-tabs",
                    select {
                        class: "forum-sort-select",
                        value: current_sort.value(),
                        onchange: move |e| sort.set(ForumSort::from_value(&e.value())),
                        option { value: "hot", "Hot" }
                        option { value: "active", "Active" }
                        option { value: "scaled", "Scaled" }
                        option { value: "controversial", "Controversial" }
                        option { value: "new", "New" }
                        option { value: "old", "Old" }
                        option { value: "most_comments", "Most Comments" }
                        option { value: "new_comments", "New Comments" }
                        option { disabled: true, value: "", "──────────────" }
                        option { value: "top_hour", "Top Hour" }
                        option { value: "top_six_hours", "Top Six Hours" }
                        option { value: "top_twelve_hours", "Top Twelve Hours" }
                        option { value: "top_day", "Top Day" }
                        option { value: "top_week", "Top Week" }
                        option { value: "top_month", "Top Month" }
                        option { value: "top_three_months", "Top Three Months" }
                        option { value: "top_six_months", "Top Six Months" }
                        option { value: "top_nine_months", "Top Nine Months" }
                        option { value: "top_year", "Top Year" }
                        option { value: "top_all_time", "Top All Time" }
                    }
                    button {
                        class: "forum-refresh-btn",
                        title: "Refresh posts",
                        onclick: {
                            let account_id2 = app_state.read().nav.active_account_id.clone();
                            let b = account_id2.as_deref()
                                .and_then(|aid| client_manager.read().get_backend(aid));
                            let channel_id = app_state.read().nav.selected_channel.clone().unwrap_or_default();
                            move |_| {
                                if let Some(ref b) = b {
                                    let b = b.clone();
                                    let cid = channel_id.clone();
                                    let mut cd = chat_data;
                                    spawn(async move {
                                        let msgs = b.read().await
                                            .get_messages(&cid, poly_client::MessageQuery::default())
                                            .await
                                            .unwrap_or_default();
                                        cd.write().messages = msgs;
                                    });
                                }
                            }
                        },
                        "↻"
                    }
                }
            }

            // Post list
            div { class: "forum-post-list",
                if visible_posts.is_empty() {
                    div { class: "forum-empty",
                        div { class: "forum-empty-icon", "📋" }
                        p { "No posts yet." }
                    }
                }
                for post in visible_posts {
                    {
                        let post2 = post.clone();
                        let post_id = post.id.clone();
                        let (backend, instance_id, account_id2, server_id, channel_id) = route_params.clone();
                        let nav2 = nav.clone();
                        rsx! {
                            ForumPostCard {
                                key: "{post_id}",
                                post: post2.clone(),
                                on_click: move |_| {
                                    nav2.push(Route::ForumPostRoute {
                                        backend: backend.clone(),
                                        instance_id: instance_id.clone(),
                                        account_id: account_id2.clone(),
                                        server_id: server_id.clone(),
                                        channel_id: channel_id.clone(),
                                        post_id: post_id.clone(),
                                    });
                                },
                            }
                        }
                    }
                }
                // Load-more sentinel: visible when there are more posts; clicking loads next page.
                if has_more {
                    div {
                        class: "forum-load-more",
                        id: "forum-scroll-sentinel",
                        onclick: move |_| visible_count.set(vc + PAGE_SIZE),
                        "Load more ({total_posts - vc} remaining)"
                    }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ForumPostView — route component: load + render single post + comments
// ─────────────────────────────────────────────────────────────────────────────

#[rustfmt::skip]
#[component]
pub fn ForumPostView(channel_id: String, post_id: String) -> Element {
    let chat_data: Signal<ChatData> = use_context();
    let app_state: Signal<AppState> = use_context();
    let client_manager: Signal<ClientManager> = use_context();

    let mut thread_comments: Signal<Vec<Message>> = use_signal(Vec::new);
    let mut thread_loading: Signal<bool> = use_signal(|| true);

    // Load channel data + comments on mount / when post_id changes.
    let post_id_clone = post_id.clone();
    let channel_id_clone = channel_id.clone();
    use_effect(move || {
        let pid = post_id_clone.clone();
        let cid = channel_id_clone.clone();

        // Ensure the server+channel context is loaded (handles direct URL navigation).
        let server_id = app_state.read().nav.selected_server.clone().unwrap_or_default();

        let account_id = app_state.read().nav.active_account_id.clone();
        let backend = account_id.as_deref()
            .and_then(|aid| client_manager.read().get_backend(aid));

        // Check if channel data is already loaded for this channel.
        let already_loaded = {
            let snap = chat_data.read();
            snap.current_channel.as_ref().is_some_and(|ch| ch.id == cid)
                && snap.current_server.as_ref().is_some_and(|s| s.id == server_id)
        };

        if !already_loaded {
            let app_state2 = app_state;
            let client_manager2 = client_manager;
            let chat_data2 = chat_data;
            let backend2 = backend.clone();
            let pid2 = pid.clone();
            spawn(async move {
                restore_server_channel(
                    server_id,
                    cid,
                    app_state2,
                    client_manager2,
                    chat_data2,
                )
                .await;
                // After channel loaded, fetch comments.
                if let Some(ref b) = backend2 {
                    let b = b.clone();
                    let comment_channel = format!("hn-post-{pid2}");
                    let result = b.read().await
                        .get_messages(&comment_channel, MessageQuery { limit: Some(200), ..Default::default() })
                        .await
                        .unwrap_or_default();
                    thread_comments.set(result);
                }
                thread_loading.set(false);
            });
        } else {
            // Channel already loaded — just fetch comments.
            if let Some(ref b) = backend {
                let b = b.clone();
                spawn(async move {
                    let comment_channel = format!("hn-post-{pid}");
                    let result = b.read().await
                        .get_messages(&comment_channel, MessageQuery { limit: Some(200), ..Default::default() })
                        .await
                        .unwrap_or_default();
                    thread_comments.set(result);
                    thread_loading.set(false);
                });
            } else {
                thread_loading.set(false);
            }
        }
    });

    // Find the post in the currently loaded messages.
    let post = chat_data.read().messages.iter()
        .find(|m| m.id == post_id)
        .cloned();

    match post {
        None => rsx! {
            div { class: "forum-post-loading",
                if *thread_loading.read() {
                    span { "Loading post…" }
                } else {
                    span { "Post not found." }
                }
            }
        },
        Some(p) => rsx! {
            ForumThreadView {
                post: p,
                comments: thread_comments.read().clone(),
                loading: *thread_loading.read(),
            }
        },
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Post card
// ─────────────────────────────────────────────────────────────────────────────

#[rustfmt::skip]
#[component]
fn ForumPostCard(post: Message, on_click: EventHandler<()>) -> Element {
    let score = post_score(&post);
    let comments_count = reaction_count(&post, "💬");
    let author_name = post.author.display_name.clone();
    let author_initial = author_name.chars().next().unwrap_or('?').to_uppercase().to_string();
    let avatar_url = post.author.avatar_url.clone();
    let author_color = user_color(&post.author.id);
    let time_str = forum_ts(post.timestamp);
    let text = post_text(&post.content).to_string();
    let sc = score_class(score);

    rsx! {
        div {
            class: "forum-post-card",
            onclick: move |_| on_click.call(()),
            div { class: "forum-post-votes",
                button { class: "forum-vote-btn up", title: "Upvote",
                    onclick: |e: MouseEvent| e.stop_propagation(),
                    "▲"
                }
                span { class: "{sc}", "{score}" }
                button { class: "forum-vote-btn down", title: "Downvote",
                    onclick: |e: MouseEvent| e.stop_propagation(),
                    "▼"
                }
            }
            div { class: "forum-post-body",
                div { class: "forum-post-author-row",
                    if let Some(ref url) = avatar_url {
                        img { class: "forum-post-avatar", src: "{url}", alt: "{author_name}" }
                    } else {
                        div {
                            class: "forum-post-avatar forum-post-avatar-initial",
                            style: "background:{author_color}",
                            "{author_initial}"
                        }
                    }
                    span { class: "forum-post-author-name", "{author_name}" }
                    span { class: "forum-post-time", "· {time_str}" }
                }
                p { class: "forum-post-content", "{text}" }
                div { class: "forum-post-footer",
                    span { class: "forum-post-comments", "💬 {comments_count} comments" }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Thread view
// ─────────────────────────────────────────────────────────────────────────────

#[rustfmt::skip]
#[component]
fn ForumThreadView(post: Message, comments: Vec<Message>, loading: bool) -> Element {
    let score = post_score(&post);
    let sc = score_class(score);
    let author_name = post.author.display_name.clone();
    let author_initial = author_name.chars().next().unwrap_or('?').to_uppercase().to_string();
    let avatar_url = post.author.avatar_url.clone();
    let author_color = user_color(&post.author.id);
    let time_str = forum_ts(post.timestamp);
    let text = post_text(&post.content).to_string();
    let n = comments.len();
    let count_label = if loading {
        "Loading comments…".to_string()
    } else if n == 0 {
        "No comments yet".to_string()
    } else if n == 1 {
        "1 comment".to_string()
    } else {
        format!("{n} comments")
    };
    let tree = build_comment_tree(&post.id, &comments);

    rsx! {
        div { class: "forum-thread-view",
            // Original post
            div { class: "forum-thread-post",
                div { class: "forum-post-votes",
                    button { class: "forum-vote-btn up", "▲" }
                    span { class: "{sc}", "{score}" }
                    button { class: "forum-vote-btn down", "▼" }
                }
                div { class: "forum-post-body",
                    div { class: "forum-post-author-row",
                        if let Some(ref url) = avatar_url {
                            img { class: "forum-post-avatar", src: "{url}", alt: "{author_name}" }
                        } else {
                            div {
                                class: "forum-post-avatar forum-post-avatar-initial",
                                style: "background:{author_color}",
                                "{author_initial}"
                            }
                        }
                        span { class: "forum-post-author-name", "{author_name}" }
                        span { class: "forum-post-time", "· {time_str}" }
                    }
                    p { class: "forum-thread-post-content", "{text}" }
                }
            }
            // Comment count header
            div { class: "forum-comments-header",
                span { class: "forum-comments-count", "{count_label}" }
            }
            // Comment tree
            div { class: "forum-comment-list",
                for node in tree {
                    ForumComment { node: node.clone() }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Recursive comment component (named ForumComment to avoid struct/component clash)
// ─────────────────────────────────────────────────────────────────────────────

#[rustfmt::skip]
#[component]
fn ForumComment(node: ForumCommentNode) -> Element {
    let msg = &node.msg;
    let depth = node.depth;
    let children = node.children.clone();

    let mut collapsed = use_signal(|| false);

    let score = post_score(msg);
    let sc = score_class(score);
    let show_score = msg.author.backend.slug() != "hackernews" && score != 0;
    let author_name = msg.author.display_name.clone();
    let author_initial = author_name.chars().next().unwrap_or('?').to_uppercase().to_string();
    let avatar_url = msg.author.avatar_url.clone();
    let author_color = user_color(&msg.author.id);
    let time_str = forum_ts(msg.timestamp);
    let text = post_text(&msg.content).to_string();
    let score_label = format!("{score:+}");

    let indent_px = (depth.min(4) * 20) as i32;
    let border_color = match depth % 4 {
        0 => "#60a5fa",
        1 => "#4ade80",
        2 => "#fbbf24",
        _ => "#a78bfa",
    };

    // Count total descendants for the collapsed summary.
    fn count_descendants(nodes: &[ForumCommentNode]) -> usize {
        nodes.iter().fold(0, |acc, n| acc + 1 + count_descendants(&n.children))
    }
    let descendant_count = count_descendants(&children);
    let is_collapsed = *collapsed.read();
    let toggle_label = if is_collapsed { "[+]" } else { "[-]" };
    let collapsed_hint = if is_collapsed && descendant_count > 0 {
        format!(" ({descendant_count} hidden)")
    } else {
        String::new()
    };

    rsx! {
        div {
            class: "forum-comment",
            style: "margin-left: {indent_px}px; border-left: 2px solid {border_color};",
            div { class: "forum-comment-header",
                button {
                    class: "forum-comment-collapse",
                    onclick: move |_| collapsed.set(!is_collapsed),
                    "{toggle_label}"
                }
                if let Some(ref url) = avatar_url {
                    img { class: "forum-comment-avatar", src: "{url}", alt: "{author_name}" }
                } else {
                    div {
                        class: "forum-comment-avatar forum-comment-avatar-initial",
                        style: "background:{author_color}",
                        "{author_initial}"
                    }
                }
                span { class: "forum-comment-author", "{author_name}" }
                span { class: "forum-comment-time", "· {time_str}" }
                if show_score {
                    span { class: "{sc} forum-comment-score", "{score_label}" }
                }
                if !collapsed_hint.is_empty() {
                    span { class: "forum-comment-collapsed-hint", "{collapsed_hint}" }
                }
            }
            if !is_collapsed {
                p { class: "forum-comment-body", "{text}" }
                for child in children {
                    ForumComment { node: child.clone() }
                }
            }
        }
    }
}
