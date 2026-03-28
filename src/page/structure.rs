use std::collections::HashMap;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::protocol::messages::{RawNode, RawSnapshot, Rect};

const MAX_TEXT_LEN: usize = 200;
const MAX_BLOCK_RENDER_LINES: usize = 20;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageData {
    pub url: String,
    pub title: String,
    pub current_page: u32,
    pub total_pages: u32,
    pub truncated: bool,
    pub shown: usize,
    pub total: usize,
    pub nodes: Vec<Node>,
    #[serde(skip)]
    pub element_refs: HashMap<String, String>,
    #[serde(skip)]
    pub full_texts: HashMap<String, String>,
    #[serde(skip)]
    pub full_blocks: HashMap<String, StoredBlock>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Node {
    Container {
        tag: String,
        role: Option<String>,
        children: Vec<Node>,
    },
    Text {
        id: Option<String>,
        text: String,
    },
    Heading {
        level: u8,
        text: String,
    },
    Link {
        id: String,
        text: String,
        href: Option<String>,
    },
    Button {
        id: String,
        text: String,
    },
    Input {
        id: String,
        input_type: String,
        placeholder: Option<String>,
        value: Option<String>,
        disabled: bool,
    },
    Checkbox {
        id: String,
        text: String,
        checked: bool,
    },
    Radio {
        id: String,
        text: String,
        name: Option<String>,
        selected: bool,
    },
    Select {
        id: String,
        text: String,
        selected: Option<String>,
        disabled: bool,
    },
    Textarea {
        id: String,
        text: String,
        placeholder: Option<String>,
        disabled: bool,
    },
    List {
        id: Option<String>,
        truncated: bool,
        shown: usize,
        total_items: usize,
        current_page: u32,
        total_pages: u32,
        children: Vec<Node>,
    },
    Item {
        children: Vec<Node>,
    },
    Table {
        id: Option<String>,
        truncated: bool,
        shown: usize,
        total_items: usize,
        current_page: u32,
        total_pages: u32,
        children: Vec<Node>,
    },
    Row {
        children: Vec<Node>,
    },
    Cell {
        children: Vec<Node>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BlockData {
    List {
        id: String,
        truncated: bool,
        shown: usize,
        total_items: usize,
        current_page: u32,
        total_pages: u32,
        children: Vec<Node>,
    },
    Table {
        id: String,
        truncated: bool,
        shown: usize,
        total_items: usize,
        current_page: u32,
        total_pages: u32,
        children: Vec<Node>,
    },
}

#[derive(Debug, Clone)]
pub enum StoredBlock {
    List { items: Vec<Node> },
    Table { rows: Vec<Node> },
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchMatch {
    pub page: u32,
    pub element_id: Option<String>,
    pub ref_id: String,
    pub tag: String,
    pub text: String,
    pub context: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchResults {
    pub query: String,
    pub matches: Vec<SearchMatch>,
}

#[derive(Debug, Clone)]
enum InteractiveKind {
    Link {
        text: String,
        href: Option<String>,
    },
    Button {
        text: String,
    },
    Input {
        text: String,
        input_type: String,
        placeholder: Option<String>,
        value: Option<String>,
        disabled: bool,
    },
    Checkbox {
        text: String,
        checked: bool,
    },
    Radio {
        text: String,
        name: Option<String>,
        selected: bool,
    },
    Select {
        text: String,
        selected: Option<String>,
        disabled: bool,
    },
    Textarea {
        text: String,
        placeholder: Option<String>,
        disabled: bool,
    },
}

#[derive(Debug, Clone)]
struct BuildState {
    next_element_id: usize,
    next_text_id: usize,
    next_block_id: usize,
    element_refs: HashMap<String, String>,
    full_texts: HashMap<String, String>,
    full_blocks: HashMap<String, StoredBlock>,
}

#[derive(Debug, Clone)]
struct ViewFilter {
    page_top: f64,
    page_bottom: f64,
}

pub fn truncate_text(text: &str, max_len: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max_len {
        text.to_string()
    } else {
        let truncated: String = chars[..max_len].iter().collect();
        format!("{truncated}[...truncated]")
    }
}

pub fn parse_snapshot(value: &serde_json::Value) -> Result<RawSnapshot> {
    serde_json::from_value(value.clone()).context("invalid raw snapshot payload")
}

#[allow(dead_code)]
pub fn parse_page(value: &serde_json::Value, requested_page: Option<u32>) -> Result<PageData> {
    let snapshot = parse_snapshot(value)?;
    parse_page_from_snapshot(&snapshot, requested_page)
}

pub fn parse_page_from_snapshot(
    snapshot: &RawSnapshot,
    requested_page: Option<u32>,
) -> Result<PageData> {
    let viewport_height = snapshot.viewport.height.max(1.0);
    let scroll_height = snapshot.scroll.height.max(viewport_height);
    let total_pages = (scroll_height / viewport_height).ceil().max(1.0) as u32;
    let fallback_page = (snapshot.scroll.top / viewport_height).floor() as u32 + 1;
    let current_page = requested_page
        .unwrap_or(fallback_page)
        .clamp(1, total_pages.max(1));

    let page_top = (current_page.saturating_sub(1) as f64) * viewport_height;
    let page_bottom = page_top + viewport_height;
    let filter = ViewFilter {
        page_top,
        page_bottom,
    };

    let node_by_ref: HashMap<&str, &RawNode> = snapshot
        .nodes
        .iter()
        .map(|node| (node.ref_id.as_str(), node))
        .collect();
    let roots = root_refs(snapshot);
    let children_by_parent = build_children_map(snapshot);

    let mut state = BuildState {
        next_element_id: 1,
        next_text_id: 1,
        next_block_id: 1,
        element_refs: HashMap::new(),
        full_texts: HashMap::new(),
        full_blocks: HashMap::new(),
    };

    let mut nodes = Vec::new();
    for root_ref in roots {
        let Some(root) = node_by_ref.get(root_ref.as_str()).copied() else {
            continue;
        };
        if root.tag == "body" {
            nodes.extend(build_child_nodes(
                root.ref_id.as_str(),
                &children_by_parent,
                &node_by_ref,
                &filter,
                &mut state,
            ));
            continue;
        }
        if let Some(node) = build_node(root, &children_by_parent, &node_by_ref, &filter, &mut state)
        {
            nodes.push(node);
        }
    }

    let total = count_nodes(&nodes);
    let shown = total;

    Ok(PageData {
        url: snapshot.url.clone(),
        title: snapshot.title.clone(),
        current_page,
        total_pages,
        truncated: false,
        shown,
        total,
        nodes,
        element_refs: state.element_refs,
        full_texts: state.full_texts,
        full_blocks: state.full_blocks,
    })
}

pub fn resolve_block(
    page: &PageData,
    block_id: &str,
    requested_page: Option<u32>,
) -> Option<BlockData> {
    page.full_blocks
        .get(block_id)
        .map(|block| block.resolve(block_id, requested_page))
}

pub fn resolve_block_all(
    page: &PageData,
    block_id: &str,
) -> Option<BlockData> {
    page.full_blocks
        .get(block_id)
        .map(|block| block.resolve_all(block_id))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewData {
    pub target: String,
    pub url: String,
    pub title: String,
    pub context_tag: Option<String>,
    pub nodes: Vec<Node>,
}

pub fn extract_view(page: &PageData, target: &str) -> Result<ViewData> {
    // Case 1: Block ID (b1, b2, ...) — auto-expand all pages
    if target.starts_with('b') && target[1..].chars().all(|c| c.is_ascii_digit()) {
        if let Some(block) = page.full_blocks.get(target) {
            let (context_tag, nodes) = match block.resolve_all(target) {
                BlockData::List { children, .. } => (Some("list".to_string()), children),
                BlockData::Table { children, .. } => (Some("table".to_string()), children),
            };
            return Ok(ViewData {
                target: target.to_string(),
                url: page.url.clone(),
                title: page.title.clone(),
                context_tag,
                nodes,
            });
        }
    }

    // Case 2: Text ID (t1, t2, ...) — return full text
    if target.starts_with('t') && target[1..].chars().all(|c| c.is_ascii_digit()) {
        if let Some(full_text) = page.full_texts.get(target) {
            return Ok(ViewData {
                target: target.to_string(),
                url: page.url.clone(),
                title: page.title.clone(),
                context_tag: None,
                nodes: vec![Node::Text { id: Some(target.to_string()), text: full_text.clone() }],
            });
        }
    }

    // Case 3: Element ID — find the element and extract its context subtree
    // Accept both "e3" and "3" format
    let element_id = if target.starts_with('e') && target[1..].chars().all(|c| c.is_ascii_digit()) {
        target.to_string()
    } else if target.chars().all(|c| c.is_ascii_digit()) {
        format!("e{target}")
    } else {
        // Case 4: Text query — search for matching interactive element
        let found = find_view_target_by_query(&page.nodes, target)
            .ok_or_else(|| anyhow::anyhow!(
                "no element matching \"{target}\" found on the current page"
            ))?;
        found
    };

    // Extract the subtree containing this element
    let subtree = extract_subtree_for_element(&page.nodes, &element_id, &page.full_blocks);
    if subtree.is_empty() {
        anyhow::bail!("element {element_id} not found in current page tree");
    }

    let context_tag = match &subtree[0] {
        Node::Container { tag, .. } => Some(tag.clone()),
        Node::List { .. } => Some("list".to_string()),
        Node::Table { .. } => Some("table".to_string()),
        _ => None,
    };

    Ok(ViewData {
        target: element_id,
        url: page.url.clone(),
        title: page.title.clone(),
        context_tag,
        nodes: subtree,
    })
}

fn find_view_target_by_query(nodes: &[Node], query: &str) -> Option<String> {
    let needle = query.to_lowercase();
    find_view_target_recursive(nodes, &needle)
}

fn find_view_target_recursive(nodes: &[Node], needle: &str) -> Option<String> {
    for node in nodes {
        match node {
            Node::Link { id, text, href, .. } => {
                if text.to_lowercase().contains(needle)
                    || href.as_deref().unwrap_or("").to_lowercase().contains(needle)
                {
                    return Some(id.clone());
                }
            }
            Node::Button { id, text, .. } => {
                if text.to_lowercase().contains(needle) {
                    return Some(id.clone());
                }
            }
            Node::Input { id, placeholder, value, .. } => {
                if placeholder.as_deref().unwrap_or("").to_lowercase().contains(needle)
                    || value.as_deref().unwrap_or("").to_lowercase().contains(needle)
                {
                    return Some(id.clone());
                }
            }
            Node::Checkbox { id, text, .. }
            | Node::Radio { id, text, .. }
            | Node::Select { id, text, .. }
            | Node::Textarea { id, text, .. } => {
                if text.to_lowercase().contains(needle) {
                    return Some(id.clone());
                }
            }
            Node::Text { id: Some(tid), text } => {
                if text.to_lowercase().contains(needle) {
                    return Some(tid.clone());
                }
            }
            Node::Container { children, .. }
            | Node::List { children, .. }
            | Node::Item { children }
            | Node::Table { children, .. }
            | Node::Row { children }
            | Node::Cell { children } => {
                if let Some(found) = find_view_target_recursive(children, needle) {
                    return Some(found);
                }
            }
            _ => {}
        }
    }
    None
}

fn extract_subtree_for_element(nodes: &[Node], element_id: &str, full_blocks: &HashMap<String, StoredBlock>) -> Vec<Node> {
    for node in nodes {
        if let Some(result) = find_context_subtree(node, element_id, full_blocks) {
            return result;
        }
    }
    Vec::new()
}

fn find_context_subtree(node: &Node, element_id: &str, full_blocks: &HashMap<String, StoredBlock>) -> Option<Vec<Node>> {
    if node_has_id(node, element_id) {
        return Some(vec![node.clone()]);
    }

    match node {
        Node::Container { tag, children, .. } => {
            if subtree_contains_id(children, element_id) {
                if is_semantic_container(tag) {
                    return Some(vec![node.clone()]);
                }
                for child in children {
                    if let Some(result) = find_context_subtree(child, element_id, full_blocks) {
                        return Some(result);
                    }
                }
                return Some(vec![node.clone()]);
            }
        }
        Node::List { id, children, .. } => {
            if subtree_contains_id(children, element_id) {
                if let Some(block_id) = id {
                    if let Some(block) = full_blocks.get(block_id) {
                        let expanded = block.resolve_all(block_id);
                        return Some(vec![match expanded {
                            BlockData::List { id, shown, total_items, children, .. } => Node::List {
                                id: Some(id),
                                truncated: false,
                                shown,
                                total_items,
                                current_page: 1,
                                total_pages: 1,
                                children,
                            },
                            _ => node.clone(),
                        }]);
                    }
                }
                return Some(vec![node.clone()]);
            }
        }
        Node::Table { id, children, .. } => {
            if subtree_contains_id(children, element_id) {
                if let Some(block_id) = id {
                    if let Some(block) = full_blocks.get(block_id) {
                        let expanded = block.resolve_all(block_id);
                        return Some(vec![match expanded {
                            BlockData::Table { id, shown, total_items, children, .. } => Node::Table {
                                id: Some(id),
                                truncated: false,
                                shown,
                                total_items,
                                current_page: 1,
                                total_pages: 1,
                                children,
                            },
                            _ => node.clone(),
                        }]);
                    }
                }
                return Some(vec![node.clone()]);
            }
        }
        Node::Item { children }
        | Node::Row { children }
        | Node::Cell { children } => {
            if subtree_contains_id(children, element_id) {
                return Some(vec![node.clone()]);
            }
        }
        _ => {}
    }
    None
}

fn node_has_id(node: &Node, id: &str) -> bool {
    match node {
        Node::Link { id: nid, .. }
        | Node::Button { id: nid, .. }
        | Node::Input { id: nid, .. }
        | Node::Checkbox { id: nid, .. }
        | Node::Radio { id: nid, .. }
        | Node::Select { id: nid, .. }
        | Node::Textarea { id: nid, .. } => nid == id,
        Node::Text { id: Some(nid), .. } => nid == id,
        Node::List { id: Some(nid), .. } | Node::Table { id: Some(nid), .. } => nid == id,
        _ => false,
    }
}

fn subtree_contains_id(nodes: &[Node], id: &str) -> bool {
    nodes.iter().any(|n| node_contains_id(n, id))
}

fn node_contains_id(node: &Node, id: &str) -> bool {
    if node_has_id(node, id) {
        return true;
    }
    match node {
        Node::Container { children, .. }
        | Node::List { children, .. }
        | Node::Item { children }
        | Node::Table { children, .. }
        | Node::Row { children }
        | Node::Cell { children } => subtree_contains_id(children, id),
        _ => false,
    }
}

fn is_semantic_container(tag: &str) -> bool {
    matches!(
        tag,
        "section" | "article" | "nav" | "main" | "header" | "footer"
        | "aside" | "form" | "dialog" | "fieldset" | "details" | "summary"
    )
}

pub fn search_snapshot(snapshot: &RawSnapshot, query: &str) -> SearchResults {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return SearchResults {
            query: query.to_string(),
            matches: Vec::new(),
        };
    }

    let viewport_height = snapshot.viewport.height.max(1.0);
    let scroll_height = snapshot.scroll.height.max(viewport_height);
    let total_pages = (scroll_height / viewport_height).ceil().max(1.0) as u32;
    let interactive_ids = interactive_ids_by_ref(snapshot, total_pages);

    let mut matches = Vec::new();

    for node in &snapshot.nodes {
        let page = page_for_rect(&node.rect, viewport_height, total_pages);
        for field in searchable_fields(node) {
            if !field.lower.contains(&needle) {
                continue;
            }
            let context_source = if field.name == "text" {
                normalize_text(&node.text)
            } else {
                field.value.clone()
            };
            let context_lower = if field.name == "text" {
                context_source.to_lowercase()
            } else {
                field.lower.clone()
            };
            matches.push(SearchMatch {
                page,
                element_id: interactive_ids.get(&(page, node.ref_id.clone())).cloned(),
                ref_id: node.ref_id.clone(),
                tag: node.tag.clone(),
                text: normalize_text(&node.text),
                context: excerpt_around_match(&context_source, &context_lower, &needle),
            });
        }
    }

    matches.sort_by_key(|item| {
        let interactive_boost = if item.element_id.is_some() { 0 } else { 1 };
        let text_boost = if item.tag == "a" || item.tag == "button" || item.tag == "input" {
            0
        } else {
            1
        };
        (
            interactive_boost,
            text_boost,
            item.page,
            item.ref_id.clone(),
        )
    });
    matches.truncate(50);

    SearchResults {
        query: query.to_string(),
        matches,
    }
}

fn build_node<'a>(
    raw: &'a RawNode,
    children_by_parent: &HashMap<Option<&'a str>, Vec<&'a RawNode>>,
    node_by_ref: &HashMap<&'a str, &'a RawNode>,
    filter: &ViewFilter,
    state: &mut BuildState,
) -> Option<Node> {
    let visible_here = intersects_page(&raw.rect, filter.page_top, filter.page_bottom);

    if let Some(interactive) = classify_interactive(raw, node_by_ref) {
        if !visible_here {
            return None;
        }
        return Some(interactive_node(raw, interactive, state));
    }

    if let Some((level, text)) = classify_heading(raw) {
        if !visible_here {
            return None;
        }
        return Some(Node::Heading { level, text });
    }

    match raw.tag.as_str() {
        "ul" | "ol" => {
            let list = build_list_node(raw, children_by_parent, node_by_ref, filter, state)?;
            return Some(list);
        }
        "table" => {
            let table = build_table_node(raw, children_by_parent, node_by_ref, filter, state)?;
            return Some(table);
        }
        _ => {}
    }

    let children = build_child_nodes(
        raw.ref_id.as_str(),
        children_by_parent,
        node_by_ref,
        filter,
        state,
    );

    if !children.is_empty() {
        if should_emit_container(raw, &children) {
            return Some(Node::Container {
                tag: raw.tag.clone(),
                role: raw.attrs.get("role").cloned(),
                children,
            });
        }
        if children.len() == 1 {
            return Some(children.into_iter().next().expect("single child"));
        }
        return Some(Node::Container {
            tag: raw.tag.clone(),
            role: raw.attrs.get("role").cloned(),
            children,
        });
    }

    if !visible_here {
        return None;
    }

    let text = normalize_text(&raw.text);
    if text.is_empty() {
        return None;
    }

    Some(text_node(text, state))
}

fn build_child_nodes<'a>(
    parent_ref: &'a str,
    children_by_parent: &HashMap<Option<&'a str>, Vec<&'a RawNode>>,
    node_by_ref: &HashMap<&'a str, &'a RawNode>,
    filter: &ViewFilter,
    state: &mut BuildState,
) -> Vec<Node> {
    let Some(children) = children_by_parent.get(&Some(parent_ref)) else {
        return Vec::new();
    };

    let mut built = Vec::new();
    for child in children {
        if let Some(node) = build_node(child, children_by_parent, node_by_ref, filter, state) {
            built.push(node);
        }
    }
    built
}

fn build_list_node<'a>(
    raw: &'a RawNode,
    children_by_parent: &HashMap<Option<&'a str>, Vec<&'a RawNode>>,
    node_by_ref: &HashMap<&'a str, &'a RawNode>,
    filter: &ViewFilter,
    state: &mut BuildState,
) -> Option<Node> {
    let items = collect_list_items(raw, children_by_parent, node_by_ref, filter, state);
    if items.is_empty() {
        return None;
    }

    let pages = paginate_block(&items, estimate_list_page_lines);
    if pages.len() <= 1 {
        let total_items = items.len();
        return Some(Node::List {
            id: None,
            truncated: false,
            shown: total_items,
            total_items,
            current_page: 1,
            total_pages: 1,
            children: items,
        });
    }

    let block_id = format!("b{}", state.next_block_id);
    state.next_block_id += 1;
    let (current_page, total_pages, start, end) = first_block_page(&pages);
    let page_items = items[start..end].to_vec();
    state
        .full_blocks
        .insert(block_id.clone(), StoredBlock::List { items });

    Some(Node::List {
        id: Some(block_id),
        truncated: true,
        shown: end - start,
        total_items: pages.last().map(|(_, end)| *end).unwrap_or(0),
        current_page,
        total_pages,
        children: page_items,
    })
}

fn collect_list_items<'a>(
    raw: &'a RawNode,
    children_by_parent: &HashMap<Option<&'a str>, Vec<&'a RawNode>>,
    node_by_ref: &HashMap<&'a str, &'a RawNode>,
    filter: &ViewFilter,
    state: &mut BuildState,
) -> Vec<Node> {
    let mut items = Vec::new();
    collect_list_items_recursive(
        raw.ref_id.as_str(),
        children_by_parent,
        node_by_ref,
        filter,
        state,
        &mut items,
    );
    items
}

fn collect_list_items_recursive<'a>(
    parent_ref: &'a str,
    children_by_parent: &HashMap<Option<&'a str>, Vec<&'a RawNode>>,
    node_by_ref: &HashMap<&'a str, &'a RawNode>,
    filter: &ViewFilter,
    state: &mut BuildState,
    items: &mut Vec<Node>,
) {
    let Some(children) = children_by_parent.get(&Some(parent_ref)) else {
        return;
    };

    for child in children {
        match child.tag.as_str() {
            "li" => {
                if let Some(item) =
                    build_list_item(child, children_by_parent, node_by_ref, filter, state)
                {
                    items.push(item);
                }
            }
            "div" | "section" | "article" => {
                collect_list_items_recursive(
                    child.ref_id.as_str(),
                    children_by_parent,
                    node_by_ref,
                    filter,
                    state,
                    items,
                );
            }
            _ => {}
        }
    }
}

fn build_list_item<'a>(
    raw: &'a RawNode,
    children_by_parent: &HashMap<Option<&'a str>, Vec<&'a RawNode>>,
    node_by_ref: &HashMap<&'a str, &'a RawNode>,
    filter: &ViewFilter,
    state: &mut BuildState,
) -> Option<Node> {
    let visible_here = intersects_page(&raw.rect, filter.page_top, filter.page_bottom);
    let mut children = build_child_nodes(
        raw.ref_id.as_str(),
        children_by_parent,
        node_by_ref,
        filter,
        state,
    );
    if children.is_empty() && visible_here {
        let text = normalize_text(&raw.text);
        if !text.is_empty() {
            children.push(text_node(text, state));
        }
    }
    if children.is_empty() {
        None
    } else {
        Some(Node::Item { children })
    }
}

fn build_table_node<'a>(
    raw: &'a RawNode,
    children_by_parent: &HashMap<Option<&'a str>, Vec<&'a RawNode>>,
    node_by_ref: &HashMap<&'a str, &'a RawNode>,
    filter: &ViewFilter,
    state: &mut BuildState,
) -> Option<Node> {
    let rows = collect_table_rows(raw, children_by_parent, node_by_ref, filter, state);
    if rows.is_empty() {
        return None;
    }

    let pages = paginate_block(&rows, estimate_table_page_lines);
    if pages.len() <= 1 {
        let total_items = rows.len();
        return Some(Node::Table {
            id: None,
            truncated: false,
            shown: total_items,
            total_items,
            current_page: 1,
            total_pages: 1,
            children: rows,
        });
    }

    let block_id = format!("b{}", state.next_block_id);
    state.next_block_id += 1;
    let (current_page, total_pages, start, end) = first_block_page(&pages);
    let page_rows = rows[start..end].to_vec();
    let total_items = rows.len();
    state
        .full_blocks
        .insert(block_id.clone(), StoredBlock::Table { rows });

    Some(Node::Table {
        id: Some(block_id),
        truncated: true,
        shown: end - start,
        total_items,
        current_page,
        total_pages,
        children: page_rows,
    })
}

fn collect_table_rows<'a>(
    raw: &'a RawNode,
    children_by_parent: &HashMap<Option<&'a str>, Vec<&'a RawNode>>,
    node_by_ref: &HashMap<&'a str, &'a RawNode>,
    filter: &ViewFilter,
    state: &mut BuildState,
) -> Vec<Node> {
    let mut rows = Vec::new();
    collect_table_rows_recursive(
        raw.ref_id.as_str(),
        children_by_parent,
        node_by_ref,
        filter,
        state,
        &mut rows,
    );
    rows
}

fn collect_table_rows_recursive<'a>(
    parent_ref: &'a str,
    children_by_parent: &HashMap<Option<&'a str>, Vec<&'a RawNode>>,
    node_by_ref: &HashMap<&'a str, &'a RawNode>,
    filter: &ViewFilter,
    state: &mut BuildState,
    rows: &mut Vec<Node>,
) {
    let Some(children) = children_by_parent.get(&Some(parent_ref)) else {
        return;
    };

    for child in children {
        match child.tag.as_str() {
            "tr" => {
                if let Some(row) =
                    build_table_row(child, children_by_parent, node_by_ref, filter, state)
                {
                    rows.push(row);
                }
            }
            "tbody" | "thead" | "tfoot" | "div" => {
                collect_table_rows_recursive(
                    child.ref_id.as_str(),
                    children_by_parent,
                    node_by_ref,
                    filter,
                    state,
                    rows,
                );
            }
            _ => {}
        }
    }
}

fn build_table_row<'a>(
    raw: &'a RawNode,
    children_by_parent: &HashMap<Option<&'a str>, Vec<&'a RawNode>>,
    node_by_ref: &HashMap<&'a str, &'a RawNode>,
    filter: &ViewFilter,
    state: &mut BuildState,
) -> Option<Node> {
    let visible_here = intersects_page(&raw.rect, filter.page_top, filter.page_bottom);
    let Some(children) = children_by_parent.get(&Some(raw.ref_id.as_str())) else {
        return None;
    };

    let mut cells = Vec::new();
    for child in children {
        if !matches!(child.tag.as_str(), "td" | "th") {
            continue;
        }
        if let Some(cell) = build_table_cell(child, children_by_parent, node_by_ref, filter, state)
        {
            cells.push(cell);
        }
    }

    if cells.is_empty() && !visible_here {
        None
    } else {
        Some(Node::Row { children: cells })
    }
}

fn build_table_cell<'a>(
    raw: &'a RawNode,
    children_by_parent: &HashMap<Option<&'a str>, Vec<&'a RawNode>>,
    node_by_ref: &HashMap<&'a str, &'a RawNode>,
    filter: &ViewFilter,
    state: &mut BuildState,
) -> Option<Node> {
    let visible_here = intersects_page(&raw.rect, filter.page_top, filter.page_bottom);
    let mut children = build_child_nodes(
        raw.ref_id.as_str(),
        children_by_parent,
        node_by_ref,
        filter,
        state,
    );
    if children.is_empty() && visible_here {
        let text = normalize_text(&raw.text);
        if !text.is_empty() {
            children.push(text_node(text, state));
        }
    }
    if children.is_empty() {
        None
    } else {
        Some(Node::Cell { children })
    }
}

fn interactive_node(raw: &RawNode, interactive: InteractiveKind, state: &mut BuildState) -> Node {
    let id = format!("e{}", state.next_element_id);
    state.next_element_id += 1;
    state.element_refs.insert(id.clone(), raw.ref_id.clone());

    match interactive {
        InteractiveKind::Link { text, href } => Node::Link { id, text, href },
        InteractiveKind::Button { text } => Node::Button { id, text },
        InteractiveKind::Input {
            text,
            input_type,
            placeholder,
            value,
            disabled,
        } => Node::Input {
            id,
            input_type,
            placeholder,
            value: value.or(if text.is_empty() { None } else { Some(text) }),
            disabled,
        },
        InteractiveKind::Checkbox { text, checked } => Node::Checkbox { id, text, checked },
        InteractiveKind::Radio {
            text,
            name,
            selected,
        } => Node::Radio {
            id,
            text,
            name,
            selected,
        },
        InteractiveKind::Select {
            text,
            selected,
            disabled,
        } => Node::Select {
            id,
            text,
            selected,
            disabled,
        },
        InteractiveKind::Textarea {
            text,
            placeholder,
            disabled,
        } => Node::Textarea {
            id,
            text,
            placeholder,
            disabled,
        },
    }
}

fn text_node(text: String, state: &mut BuildState) -> Node {
    let truncated = truncate_text(&text, MAX_TEXT_LEN);
    let text_id = if truncated != text {
        let id = format!("t{}", state.next_text_id);
        state.next_text_id += 1;
        state.full_texts.insert(id.clone(), text);
        Some(id)
    } else {
        None
    };

    Node::Text {
        id: text_id,
        text: truncated,
    }
}

fn root_refs(snapshot: &RawSnapshot) -> Vec<String> {
    snapshot
        .nodes
        .iter()
        .filter(|node| node.parent.is_none())
        .map(|node| node.ref_id.clone())
        .collect()
}

fn build_children_map<'a>(snapshot: &'a RawSnapshot) -> HashMap<Option<&'a str>, Vec<&'a RawNode>> {
    let mut map: HashMap<Option<&str>, Vec<&RawNode>> = HashMap::new();
    for node in &snapshot.nodes {
        map.entry(node.parent.as_deref()).or_default().push(node);
    }
    map
}

fn classify_interactive<'a>(
    node: &'a RawNode,
    node_by_ref: &HashMap<&'a str, &'a RawNode>,
) -> Option<InteractiveKind> {
    if has_interactive_ancestor(node, node_by_ref) {
        return None;
    }

    let tag = node.tag.as_str();
    let text = element_label(node);
    let attrs = &node.attrs;
    let role = attrs.get("role").map(String::as_str);
    let onclick = attrs.get("onclick").is_some();

    match tag {
        "a" => Some(InteractiveKind::Link {
            text,
            href: attrs.get("href").cloned(),
        }),
        "button" => Some(InteractiveKind::Button { text }),
        "select" => Some(InteractiveKind::Select {
            text,
            selected: attrs.get("value").cloned(),
            disabled: is_trueish(attrs.get("disabled")),
        }),
        "textarea" => Some(InteractiveKind::Textarea {
            text: attrs.get("value").cloned().unwrap_or(text),
            placeholder: attrs.get("placeholder").cloned(),
            disabled: is_trueish(attrs.get("disabled")),
        }),
        "input" => {
            let input_type = attrs
                .get("type")
                .cloned()
                .unwrap_or_else(|| "text".into())
                .to_lowercase();
            match input_type.as_str() {
                "checkbox" => Some(InteractiveKind::Checkbox {
                    text,
                    checked: is_trueish(attrs.get("checked")),
                }),
                "radio" => Some(InteractiveKind::Radio {
                    text,
                    name: attrs.get("name").cloned(),
                    selected: is_trueish(attrs.get("checked")),
                }),
                "submit" | "button" | "reset" => Some(InteractiveKind::Button { text }),
                _ => Some(InteractiveKind::Input {
                    text,
                    input_type,
                    placeholder: attrs.get("placeholder").cloned(),
                    value: attrs.get("value").cloned(),
                    disabled: is_trueish(attrs.get("disabled")),
                }),
            }
        }
        _ if role == Some("button") || onclick => Some(InteractiveKind::Button { text }),
        _ => None,
    }
}

fn classify_heading(node: &RawNode) -> Option<(u8, String)> {
    let tag = node.tag.as_str();
    if tag.len() == 2 && tag.starts_with('h') {
        let level = tag[1..].parse::<u8>().ok()?;
        if (1..=6).contains(&level) {
            let text = normalize_text(&node.text);
            if !text.is_empty() {
                return Some((level, text));
            }
        }
    }
    None
}

fn has_interactive_ancestor<'a>(
    node: &'a RawNode,
    node_by_ref: &HashMap<&'a str, &'a RawNode>,
) -> bool {
    let mut current = node.parent.as_deref();
    while let Some(parent_ref) = current {
        let Some(parent) = node_by_ref.get(parent_ref).copied() else {
            break;
        };
        if is_interactive_tag(parent.tag.as_str(), &parent.attrs) {
            return true;
        }
        current = parent.parent.as_deref();
    }
    false
}

fn is_interactive_tag(tag: &str, attrs: &HashMap<String, String>) -> bool {
    matches!(tag, "a" | "button" | "input" | "select" | "textarea")
        || attrs.get("role").map(String::as_str) == Some("button")
        || attrs.contains_key("onclick")
}

fn should_emit_container(raw: &RawNode, children: &[Node]) -> bool {
    if raw.attrs.contains_key("role") {
        return true;
    }
    if matches!(
        raw.tag.as_str(),
        "section"
            | "article"
            | "nav"
            | "main"
            | "header"
            | "footer"
            | "aside"
            | "form"
            | "dialog"
            | "fieldset"
            | "details"
            | "summary"
    ) {
        return true;
    }
    if matches!(raw.tag.as_str(), "div" | "span") {
        return children.len() > 1;
    }
    !children.is_empty()
}

fn searchable_fields(node: &RawNode) -> Vec<SearchField<'_>> {
    let mut fields = Vec::new();
    let text = normalize_text(&node.text);
    if !text.is_empty() {
        fields.push(SearchField {
            name: "text",
            lower: text.to_lowercase(),
            value: text,
        });
    }
    for attr in [
        "href",
        "placeholder",
        "value",
        "aria-label",
        "name",
        "title",
    ] {
        if let Some(value) = node.attrs.get(attr) {
            let normalized = normalize_text(value);
            if normalized.is_empty() {
                continue;
            }
            fields.push(SearchField {
                name: attr,
                lower: normalized.to_lowercase(),
                value: normalized,
            });
        }
    }
    fields
}

#[derive(Debug, Clone)]
struct SearchField<'a> {
    name: &'a str,
    lower: String,
    value: String,
}

fn interactive_ids_by_ref(
    snapshot: &RawSnapshot,
    total_pages: u32,
) -> HashMap<(u32, String), String> {
    let mut ids = HashMap::new();
    for page in 1..=total_pages.max(1) {
        if let Ok(page_data) = parse_page_from_snapshot(snapshot, Some(page)) {
            for (element_id, ref_id) in page_data.element_refs {
                ids.insert((page, ref_id), element_id);
            }
        }
    }
    ids
}

fn page_for_rect(rect: &Rect, viewport_height: f64, total_pages: u32) -> u32 {
    ((rect.y / viewport_height).floor() as u32 + 1).clamp(1, total_pages.max(1))
}

fn element_label(node: &RawNode) -> String {
    let text = normalize_text(&node.text);
    if !text.is_empty() {
        return text;
    }
    node.attrs
        .get("aria-label")
        .or_else(|| node.attrs.get("title"))
        .or_else(|| node.attrs.get("placeholder"))
        .cloned()
        .unwrap_or_default()
}

fn normalize_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn excerpt_around_match(text: &str, lower: &str, needle: &str) -> String {
    let Some(idx) = lower.find(needle) else {
        return truncate_text(text, MAX_TEXT_LEN);
    };
    let start = idx.saturating_sub(40);
    let end = (idx + needle.len() + 80).min(text.len());
    truncate_text(text[start..end].trim(), MAX_TEXT_LEN)
}

fn intersects_page(rect: &Rect, page_top: f64, page_bottom: f64) -> bool {
    let rect_top = rect.y;
    let rect_bottom = rect.y + rect.h;
    rect_bottom > page_top && rect_top < page_bottom
}

fn is_trueish(value: Option<&String>) -> bool {
    matches!(
        value.map(String::as_str),
        Some("true" | "checked" | "disabled" | "selected" | "1" | "")
    )
}

fn count_nodes(nodes: &[Node]) -> usize {
    nodes.iter().map(count_node).sum()
}

fn count_node(node: &Node) -> usize {
    1 + match node {
        Node::Container { children, .. }
        | Node::List { children, .. }
        | Node::Item { children }
        | Node::Table { children, .. }
        | Node::Row { children }
        | Node::Cell { children } => count_nodes(children),
        _ => 0,
    }
}

fn estimate_list_page_lines(items: &[Node]) -> usize {
    2 + items.iter().map(estimate_item_lines).sum::<usize>()
}

fn estimate_item_lines(item: &Node) -> usize {
    match item {
        Node::Item { children } => 1 + children.iter().map(estimate_node_lines).sum::<usize>(),
        other => estimate_node_lines(other),
    }
}

fn estimate_table_page_lines(rows: &[Node]) -> usize {
    2 + rows.iter().map(estimate_row_lines).sum::<usize>()
}

fn estimate_row_lines(row: &Node) -> usize {
    match row {
        Node::Row { children } => 2 + children.iter().map(estimate_cell_lines).sum::<usize>(),
        other => estimate_node_lines(other),
    }
}

fn estimate_cell_lines(cell: &Node) -> usize {
    match cell {
        Node::Cell { children } => {
            if children.len() == 1 && matches!(children[0], Node::Text { .. }) {
                1
            } else {
                2 + children.iter().map(estimate_node_lines).sum::<usize>()
            }
        }
        other => estimate_node_lines(other),
    }
}

fn estimate_node_lines(node: &Node) -> usize {
    match node {
        Node::Text { .. }
        | Node::Heading { .. }
        | Node::Link { .. }
        | Node::Button { .. }
        | Node::Input { .. }
        | Node::Checkbox { .. }
        | Node::Radio { .. }
        | Node::Select { .. }
        | Node::Textarea { .. } => 1,
        Node::Container { children, .. }
        | Node::List { children, .. }
        | Node::Item { children }
        | Node::Table { children, .. }
        | Node::Row { children }
        | Node::Cell { children } => 2 + children.iter().map(estimate_node_lines).sum::<usize>(),
    }
}

fn paginate_block<T: Clone>(
    items: &[T],
    rendered_lines_for_range: impl Fn(&[T]) -> usize,
) -> Vec<(usize, usize)> {
    if items.is_empty() {
        return Vec::new();
    }

    let mut pages = Vec::new();
    let mut start = 0usize;

    while start < items.len() {
        let mut end = start;
        while end < items.len() {
            let candidate_end = end + 1;
            let lines = rendered_lines_for_range(&items[start..candidate_end]);
            if lines <= MAX_BLOCK_RENDER_LINES || end == start {
                end = candidate_end;
                if lines > MAX_BLOCK_RENDER_LINES {
                    break;
                }
            } else {
                break;
            }
        }
        pages.push((start, end));
        start = end;
    }

    pages
}

fn first_block_page(pages: &[(usize, usize)]) -> (u32, u32, usize, usize) {
    selected_block_page(pages, Some(1))
}

fn selected_block_page(
    pages: &[(usize, usize)],
    requested_page: Option<u32>,
) -> (u32, u32, usize, usize) {
    let total_pages = pages.len().max(1) as u32;
    let current_page = requested_page.unwrap_or(1).clamp(1, total_pages);
    let (start, end) = pages
        .get(current_page.saturating_sub(1) as usize)
        .copied()
        .unwrap_or((0, 0));
    (current_page, total_pages, start, end)
}

impl StoredBlock {
    fn resolve_all(&self, block_id: &str) -> BlockData {
        match self {
            Self::List { items } => BlockData::List {
                id: block_id.to_string(),
                truncated: false,
                shown: items.len(),
                total_items: items.len(),
                current_page: 1,
                total_pages: 1,
                children: items.clone(),
            },
            Self::Table { rows } => BlockData::Table {
                id: block_id.to_string(),
                truncated: false,
                shown: rows.len(),
                total_items: rows.len(),
                current_page: 1,
                total_pages: 1,
                children: rows.clone(),
            },
        }
    }

    fn resolve(&self, block_id: &str, requested_page: Option<u32>) -> BlockData {
        match self {
            Self::List { items } => {
                let pages = paginate_block(items, estimate_list_page_lines);
                let (current_page, total_pages, start, end) =
                    selected_block_page(&pages, requested_page);
                BlockData::List {
                    id: block_id.to_string(),
                    truncated: total_pages > 1,
                    shown: end - start,
                    total_items: items.len(),
                    current_page,
                    total_pages,
                    children: items[start..end].to_vec(),
                }
            }
            Self::Table { rows } => {
                let pages = paginate_block(rows, estimate_table_page_lines);
                let (current_page, total_pages, start, end) =
                    selected_block_page(&pages, requested_page);
                BlockData::Table {
                    id: block_id.to_string(),
                    truncated: total_pages > 1,
                    shown: end - start,
                    total_items: rows.len(),
                    current_page,
                    total_pages,
                    children: rows[start..end].to_vec(),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::messages::{ScrollState, Viewport};

    fn snapshot(nodes: Vec<RawNode>) -> RawSnapshot {
        RawSnapshot {
            url: "https://example.com".into(),
            title: "Example".into(),
            viewport: Viewport {
                width: 1280.0,
                height: 800.0,
            },
            scroll: ScrollState {
                top: 0.0,
                height: 1600.0,
            },
            nodes,
        }
    }

    fn node(ref_id: &str, parent: Option<&str>, tag: &str, text: &str, y: f64) -> RawNode {
        RawNode {
            ref_id: ref_id.into(),
            parent: parent.map(str::to_string),
            tag: tag.into(),
            text: text.into(),
            attrs: HashMap::new(),
            rect: Rect {
                x: 0.0,
                y,
                w: 100.0,
                h: 20.0,
            },
        }
    }

    #[test]
    fn parse_page_builds_tree_and_assigns_text_ids() {
        let body = node("r1", None, "body", "", 0.0);
        let h1 = node("r2", Some("r1"), "h1", "Welcome", 10.0);
        let text = node(
            "r3",
            Some("r1"),
            "div",
            "This is a long paragraph that should be truncated because it is far beyond the two hundred character limit used by the CLI rendering layer. The full text should still be available from the hidden text store for the text command.",
            40.0,
        );

        let page = parse_page_from_snapshot(&snapshot(vec![body, h1, text]), Some(1)).unwrap();
        assert_eq!(page.nodes.len(), 2);
        assert!(matches!(page.nodes[0], Node::Heading { .. }));
        match &page.nodes[1] {
            Node::Text { id, .. } => assert_eq!(id.as_deref(), Some("t1")),
            other => panic!("unexpected: {other:?}"),
        }
        assert!(page.full_texts.contains_key("t1"));
    }

    #[test]
    fn parse_page_keeps_container_context() {
        let body = node("r1", None, "body", "", 0.0);
        let section = node("r2", Some("r1"), "section", "", 10.0);
        let text = node("r3", Some("r2"), "span", "Alpha", 10.0);
        let mut link = node("r4", Some("r2"), "a", "Docs", 30.0);
        link.attrs.insert("href".into(), "/docs".into());

        let page =
            parse_page_from_snapshot(&snapshot(vec![body, section, text, link]), Some(1)).unwrap();

        match &page.nodes[0] {
            Node::Container { tag, children, .. } => {
                assert_eq!(tag, "section");
                assert_eq!(children.len(), 2);
                assert!(matches!(children[1], Node::Link { .. }));
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn table_keeps_nested_link_and_mapping() {
        let body = node("r1", None, "body", "", 0.0);
        let table = node("r2", Some("r1"), "table", "", 10.0);
        let tbody = node("r3", Some("r2"), "tbody", "", 10.0);
        let tr = node("r4", Some("r3"), "tr", "", 10.0);
        let td_rank = node("r5", Some("r4"), "td", "1", 10.0);
        let td_name = node("r6", Some("r4"), "td", "Red Desert", 10.0);
        let mut link = node("r7", Some("r6"), "a", "Red Desert", 10.0);
        link.attrs.insert("href".into(), "/app/1".into());

        let page = parse_page_from_snapshot(
            &snapshot(vec![body, table, tbody, tr, td_rank, td_name, link]),
            Some(1),
        )
        .unwrap();

        match &page.nodes[0] {
            Node::Table { children, .. } => match &children[0] {
                Node::Row { children } => match &children[1] {
                    Node::Cell { children } => match &children[0] {
                        Node::Link { id, text, href } => {
                            assert_eq!(id, "e1");
                            assert_eq!(text, "Red Desert");
                            assert_eq!(href.as_deref(), Some("/app/1"));
                        }
                        other => panic!("unexpected: {other:?}"),
                    },
                    other => panic!("unexpected cell: {other:?}"),
                },
                other => panic!("unexpected row: {other:?}"),
            },
            other => panic!("unexpected table: {other:?}"),
        }
        assert_eq!(page.element_refs.get("e1").map(String::as_str), Some("r7"));
    }

    #[test]
    fn long_list_assigns_block_ids() {
        let body = node("r1", None, "body", "", 0.0);
        let ul = node("r2", Some("r1"), "ul", "", 10.0);
        let mut nodes = vec![body, ul];
        for index in 0..25 {
            nodes.push(node(
                &format!("r{}", index + 3),
                Some("r2"),
                "li",
                &format!("Item {}", index + 1),
                20.0 + index as f64 * 10.0,
            ));
        }

        let page = parse_page_from_snapshot(&snapshot(nodes), Some(1)).unwrap();
        let block_id = match &page.nodes[0] {
            Node::List { id, truncated, .. } => {
                assert!(*truncated);
                id.clone().expect("block id")
            }
            other => panic!("unexpected: {other:?}"),
        };

        let block = resolve_block(&page, &block_id, Some(2)).unwrap();
        match block {
            BlockData::List { children, .. } => assert!(!children.is_empty()),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn search_snapshot_returns_interactive_ids() {
        let body = node("r1", None, "body", "", 0.0);
        let mut link = node("r2", Some("r1"), "a", "Continue", 10.0);
        link.attrs.insert("href".into(), "/next".into());
        let result = search_snapshot(&snapshot(vec![body, link]), "continue");
        assert_eq!(result.matches[0].element_id.as_deref(), Some("e1"));
    }

    #[test]
    fn button_uses_title_as_label() {
        let body = node("r1", None, "body", "", 0.0);
        let mut button = node("r2", Some("r1"), "button", "", 10.0);
        button.attrs.insert("title".into(), "Close".into());
        let page = parse_page_from_snapshot(&snapshot(vec![body, button]), Some(1)).unwrap();
        match &page.nodes[0] {
            Node::Button { text, .. } => assert_eq!(text, "Close"),
            other => panic!("unexpected: {other:?}"),
        }
    }
}
