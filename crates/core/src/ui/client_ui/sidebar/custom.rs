//! `SidebarLayoutKind::Custom` — fully plugin-declared sections.
//!
//! Consumes `SidebarDeclaration.sections` and reconstructs the parent-id
//! tree from the flat list (analogous to [`crate::ui::client_ui::menu`]'s
//! submenu handling) before rendering. Click routing for item actions is a
//! WP 4 follow-up — for now items render as inert rows so the declared
//! tree is visible in snapshots.

use crate::i18n::t;
use dioxus::prelude::*;
use poly_client::{SidebarDeclaration, SidebarItem, SidebarSection};
use poly_ui_macros::{context_menu, ui_action};

/// Render a plugin-declared sidebar declaration.
#[ui_action(None)]
#[context_menu(inherit)]
#[component]
pub fn CustomSidebar(declaration: SidebarDeclaration) -> Element {
    let has_header_block = declaration.header_block.is_some();
    let sections = declaration.sections.clone();

    rsx! {
        aside { class: "client-sidebar custom-sidebar",
            if has_header_block {
                div { class: "custom-sidebar-header-block",
                    // WP 5 will render the sanitized CustomBlock here.
                    "[custom-block pending WP 5]"
                }
            }
            {sections.into_iter().enumerate().map(|(idx, section)| rsx! {
                CustomSidebarSection {
                    key: "{idx}",
                    section: section,
                }
            })}
        }
    }
}

/// One section (optional FTL header + a tree of items).
#[ui_action(None)]
#[context_menu(inherit)]
#[component]
fn CustomSidebarSection(section: SidebarSection) -> Element {
    // Resolve FTL key via the host i18n store; `t()` falls back to the raw
    // key when no translation is registered.
    let header = section.header_key.as_deref().map(t);
    let items = section.items.clone();
    let tree = reconstruct_tree(items);

    rsx! {
        section { class: "custom-sidebar-section",
            if let Some(h) = header {
                h3 { class: "custom-sidebar-section-header", "{h}" }
            }
            ul { class: "custom-sidebar-items",
                {tree.into_iter().enumerate().map(|(idx, node)| rsx! {
                    SidebarItemRow {
                        key: "{idx}",
                        node: node,
                        depth: 0,
                    }
                })}
            }
        }
    }
}

/// Parent/children pair produced by [`reconstruct_tree`].
#[derive(Debug, Clone, PartialEq)]
struct SidebarNode {
    item: SidebarItem,
    children: Vec<SidebarNode>,
}

/// Build a parent → children tree from a flat list keyed by
/// [`SidebarItem::parent_id`].
fn reconstruct_tree(items: Vec<SidebarItem>) -> Vec<SidebarNode> {
    use std::collections::HashMap;
    let ids: std::collections::HashSet<String> =
        items.iter().map(|i| i.id.clone()).collect();
    // First pass: bucket items by parent_id so lookups are O(1).
    let mut children_by_parent: HashMap<String, Vec<SidebarItem>> = HashMap::new();
    let mut roots: Vec<SidebarItem> = Vec::new();
    for item in items {
        match &item.parent_id {
            None => roots.push(item),
            Some(pid) => {
                if ids.contains(pid) {
                    children_by_parent.entry(pid.clone()).or_default().push(item);
                } else {
                    tracing::warn!(
                        "CustomSidebar: dropping item {:?} with unknown parent_id {:?}",
                        item.id,
                        pid
                    );
                }
            }
        }
    }
    roots
        .into_iter()
        .map(|r| build_node(r, &mut children_by_parent))
        .collect()
}

fn build_node(
    item: SidebarItem,
    children_by_parent: &mut std::collections::HashMap<String, Vec<SidebarItem>>,
) -> SidebarNode {
    let children = children_by_parent
        .remove(&item.id)
        .unwrap_or_default()
        .into_iter()
        .map(|c| build_node(c, children_by_parent))
        .collect();
    SidebarNode { item, children }
}

/// Render one row with its own children recursively.
#[ui_action(None)]
#[context_menu(inherit)]
#[component]
fn SidebarItemRow(node: SidebarNode, depth: usize) -> Element {
    let label = t(&node.item.label_key);
    let badge = node.item.badge.clone();
    let children = node.children.clone();
    let has_children = !children.is_empty();
    let indent = format!("padding-left: {}px;", 8 + depth.saturating_mul(12));

    rsx! {
        li { class: "custom-sidebar-item",
            div {
                class: "custom-sidebar-item-row",
                style: "{indent}",
                span { class: "custom-sidebar-item-label", "{label}" }
                if let Some(b) = badge {
                    span { class: "custom-sidebar-item-badge", "{b}" }
                }
            }
            if has_children {
                ul { class: "custom-sidebar-children",
                    {children.into_iter().enumerate().map(|(idx, child)| rsx! {
                        SidebarItemRow {
                            key: "{idx}",
                            node: child,
                            depth: depth + 1,
                        }
                    })}
                }
            }
        }
    }
}
