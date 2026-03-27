use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::page::xml::rendered_table_row_lines;
use crate::protocol::messages::{RawNode, RawSnapshot, Rect};

const MAX_TEXT_LEN: usize = 200;
const MAX_PAGE_ELEMENTS: usize = 200;
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
    pub elements: Vec<Element>,
    #[serde(skip)]
    pub element_refs: HashMap<String, String>,
    #[serde(skip)]
    pub full_texts: HashMap<String, String>,
    #[serde(skip)]
    pub full_blocks: HashMap<String, StoredBlock>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Element {
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
        items: Vec<String>,
        truncated: bool,
        shown: usize,
        total_items: usize,
        current_page: u32,
        total_pages: u32,
    },
    Table {
        id: Option<String>,
        rows: Vec<Vec<String>>,
        truncated: bool,
        shown: usize,
        total_items: usize,
        current_page: u32,
        total_pages: u32,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BlockData {
    List {
        id: String,
        items: Vec<String>,
        truncated: bool,
        shown: usize,
        total_items: usize,
        current_page: u32,
        total_pages: u32,
    },
    Table {
        id: String,
        rows: Vec<Vec<String>>,
        truncated: bool,
        shown: usize,
        total_items: usize,
        current_page: u32,
        total_pages: u32,
    },
}

#[derive(Debug, Clone)]
pub enum StoredBlock {
    List { items: Vec<String> },
    Table { rows: Vec<Vec<String>> },
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
struct ProcessedNode {
    order: usize,
    interactive: bool,
    element: Element,
    ref_id: Option<String>,
    full_text: Option<(String, String)>,
    full_block: Option<(String, StoredBlock)>,
}

#[derive(Debug, Clone)]
struct PendingText {
    order: usize,
    parent: Option<String>,
    rect: Rect,
    text: String,
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

    let node_by_ref: HashMap<&str, &RawNode> = snapshot
        .nodes
        .iter()
        .map(|node| (node.ref_id.as_str(), node))
        .collect();
    let mut child_count: HashMap<&str, usize> = HashMap::new();
    let mut children_by_parent: HashMap<&str, Vec<&RawNode>> = HashMap::new();
    for node in &snapshot.nodes {
        if let Some(parent) = node.parent.as_deref() {
            *child_count.entry(parent).or_insert(0) += 1;
            children_by_parent.entry(parent).or_default().push(node);
        }
    }

    let mut next_element_id = 1usize;
    let mut next_text_id = 1usize;
    let mut next_block_id = 1usize;
    let mut processed = Vec::new();
    let mut pending_text: Option<PendingText> = None;
    let mut consumed: HashSet<&str> = HashSet::new();

    for (order, node) in snapshot.nodes.iter().enumerate() {
        if !intersects_page(&node.rect, page_top, page_bottom) {
            continue;
        }

        if consumed.contains(node.ref_id.as_str()) {
            continue;
        }

        if let Some(interactive) = classify_interactive(node, &node_by_ref) {
            flush_pending_text(&mut pending_text, &mut processed, &mut next_text_id);
            let id = format!("e{next_element_id}");
            next_element_id += 1;
            processed.push(ProcessedNode {
                order,
                interactive: true,
                element: interactive.into_element(id),
                ref_id: Some(node.ref_id.clone()),
                full_text: None,
                full_block: None,
            });
            continue;
        }

        if has_interactive_ancestor(node, &node_by_ref) {
            continue;
        }

        if matches!(node.tag.as_str(), "ul" | "ol") {
            flush_pending_text(&mut pending_text, &mut processed, &mut next_text_id);
            let items = collect_list_items(&node.ref_id, &children_by_parent, &mut consumed);
            consumed.insert(node.ref_id.as_str());
            if !items.is_empty() {
                let (element, full_block) = build_list_element(items, &mut next_block_id);
                processed.push(ProcessedNode {
                    order,
                    interactive: false,
                    element,
                    ref_id: None,
                    full_text: None,
                    full_block,
                });
            }
            continue;
        }

        if node.tag == "table" {
            flush_pending_text(&mut pending_text, &mut processed, &mut next_text_id);
            let rows = collect_table_rows(&node.ref_id, &children_by_parent, &mut consumed);
            consumed.insert(node.ref_id.as_str());
            if !rows.is_empty() {
                let (element, full_block) = build_table_element(rows, &mut next_block_id);
                processed.push(ProcessedNode {
                    order,
                    interactive: false,
                    element,
                    ref_id: None,
                    full_text: None,
                    full_block,
                });
            }
            continue;
        }

        if let Some((level, text)) = classify_heading(node) {
            flush_pending_text(&mut pending_text, &mut processed, &mut next_text_id);
            processed.push(ProcessedNode {
                order,
                interactive: false,
                element: Element::Heading { level, text },
                ref_id: None,
                full_text: None,
                full_block: None,
            });
            continue;
        }

        let normalized = normalize_text(&node.text);
        if normalized.is_empty() || !should_emit_text(node, &normalized, &child_count) {
            continue;
        }

        match &mut pending_text {
            Some(pending)
                if pending.parent == node.parent
                    && pending.order + 1 == order
                    && pending.text != normalized =>
            {
                pending.text = format!("{} {}", pending.text, normalized);
                pending.rect = merge_rect(&pending.rect, &node.rect);
            }
            Some(_) => {
                flush_pending_text(&mut pending_text, &mut processed, &mut next_text_id);
                pending_text = Some(PendingText {
                    order,
                    parent: node.parent.clone(),
                    rect: node.rect.clone(),
                    text: normalized,
                });
            }
            None => {
                pending_text = Some(PendingText {
                    order,
                    parent: node.parent.clone(),
                    rect: node.rect.clone(),
                    text: normalized,
                });
            }
        }
    }

    flush_pending_text(&mut pending_text, &mut processed, &mut next_text_id);

    let total = processed.len();
    let truncated = total > MAX_PAGE_ELEMENTS;
    let selected = if truncated {
        truncate_processed(processed)
    } else {
        let mut nodes = processed;
        nodes.sort_by_key(|node| node.order);
        nodes
    };

    let mut elements = Vec::with_capacity(selected.len());
    let mut element_refs = HashMap::new();
    let mut full_texts = HashMap::new();
    let mut full_blocks = HashMap::new();

    for node in selected {
        if let Some(ref_id) = node.ref_id {
            let element_id = extract_element_id(&node.element);
            element_refs.insert(element_id, ref_id);
        }
        if let Some((text_id, full_text)) = node.full_text {
            full_texts.insert(text_id, full_text);
        }
        if let Some((block_id, block)) = node.full_block {
            full_blocks.insert(block_id, block);
        }
        elements.push(node.element);
    }

    Ok(PageData {
        url: snapshot.url.clone(),
        title: snapshot.title.clone(),
        current_page,
        total_pages,
        truncated,
        shown: elements.len(),
        total,
        elements,
        element_refs,
        full_texts,
        full_blocks,
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

pub fn search_snapshot(snapshot: &RawSnapshot, query: &str) -> SearchResults {
    let query = query.trim().to_string();
    if query.is_empty() {
        return SearchResults {
            query,
            matches: Vec::new(),
        };
    }

    let needle = query.to_lowercase();
    let viewport_height = snapshot.viewport.height.max(1.0);
    let scroll_height = snapshot.scroll.height.max(viewport_height);
    let total_pages = (scroll_height / viewport_height).ceil().max(1.0) as u32;
    let interactive_ids = interactive_ids_by_ref(snapshot, total_pages);
    let mut seen_refs = HashSet::new();
    let mut scored_matches = Vec::new();

    for node in &snapshot.nodes {
        let page = page_for_rect(&node.rect, viewport_height, total_pages);
        let fields = searchable_fields(node);
        let Some(best_match) = fields
            .iter()
            .filter_map(|field| {
                let normalized = normalize_text(field.value);
                if normalized.is_empty() {
                    return None;
                }
                let lower = normalized.to_lowercase();
                let idx = lower.find(&needle)?;
                Some((
                    field,
                    normalized,
                    lower,
                    idx,
                    search_rank(node, field.name, idx),
                ))
            })
            .min_by_key(|(_, _, _, _, rank)| rank.clone())
        else {
            continue;
        };

        if !seen_refs.insert(node.ref_id.clone()) {
            continue;
        }

        let (field, normalized, lower, _, rank) = best_match;
        let context_source = if field.name == "text" {
            &normalized
        } else {
            field.value
        };
        let context_lower = if field.name == "text" {
            lower.as_str()
        } else {
            field.lower.as_str()
        };

        scored_matches.push((
            rank,
            SearchMatch {
                page,
                element_id: interactive_ids.get(&(page, node.ref_id.clone())).cloned(),
                ref_id: node.ref_id.clone(),
                tag: node.tag.clone(),
                text: normalize_text(&node.text),
                context: excerpt_around_match(context_source, context_lower, &needle),
            },
        ));
    }

    scored_matches.sort_by(|(left_rank, left_match), (right_rank, right_match)| {
        left_rank
            .cmp(right_rank)
            .then(left_match.page.cmp(&right_match.page))
            .then(left_match.tag.cmp(&right_match.tag))
    });

    let matches = scored_matches
        .into_iter()
        .map(|(_, item)| item)
        .take(50)
        .collect();

    SearchResults { query, matches }
}

#[derive(Debug, Clone)]
struct SearchField<'a> {
    name: &'static str,
    value: &'a str,
    lower: String,
}

fn searchable_fields(node: &RawNode) -> Vec<SearchField<'_>> {
    let mut fields = Vec::new();
    let text = normalize_text(&node.text);
    if !text.is_empty() {
        fields.push(SearchField {
            name: "text",
            lower: text.to_lowercase(),
            value: &node.text,
        });
    }

    for (name, attr) in [
        ("aria-label", "aria-label"),
        ("placeholder", "placeholder"),
        ("value", "value"),
        ("name", "name"),
        ("href", "href"),
    ] {
        if let Some(value) = node
            .attrs
            .get(attr)
            .filter(|value| !value.trim().is_empty())
        {
            fields.push(SearchField {
                name,
                lower: value.to_lowercase(),
                value,
            });
        }
    }

    fields
}

fn search_rank(node: &RawNode, field_name: &str, match_index: usize) -> (u8, u8, usize, String) {
    let interactive_boost = if is_interactive_tag(node.tag.as_str(), &node.attrs) {
        0
    } else {
        1
    };
    let field_rank = match field_name {
        "text" => 0,
        "aria-label" => 1,
        "placeholder" => 2,
        "value" => 3,
        "name" => 4,
        "href" => 5,
        _ => 9,
    };

    (
        interactive_boost,
        field_rank,
        match_index,
        node.ref_id.clone(),
    )
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

impl InteractiveKind {
    fn into_element(self, id: String) -> Element {
        match self {
            Self::Link { text, href } => Element::Link { id, text, href },
            Self::Button { text } => Element::Button { id, text },
            Self::Input {
                text,
                input_type,
                placeholder,
                value,
                disabled,
            } => Element::Input {
                id,
                input_type,
                placeholder,
                value: value.or(if text.is_empty() { None } else { Some(text) }),
                disabled,
            },
            Self::Checkbox { text, checked } => Element::Checkbox { id, text, checked },
            Self::Radio {
                text,
                name,
                selected,
            } => Element::Radio {
                id,
                text,
                name,
                selected,
            },
            Self::Select {
                text,
                selected,
                disabled,
            } => Element::Select {
                id,
                text,
                selected,
                disabled,
            },
            Self::Textarea {
                text,
                placeholder,
                disabled,
            } => Element::Textarea {
                id,
                text,
                placeholder,
                disabled,
            },
        }
    }
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

fn should_emit_text(
    node: &RawNode,
    normalized_text: &str,
    child_count: &HashMap<&str, usize>,
) -> bool {
    if is_interactive_tag(node.tag.as_str(), &node.attrs) {
        return false;
    }
    if child_count.contains_key(node.ref_id.as_str()) {
        return false;
    }
    !normalized_text.is_empty()
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

fn flush_pending_text(
    pending: &mut Option<PendingText>,
    processed: &mut Vec<ProcessedNode>,
    next_text_id: &mut usize,
) {
    let Some(pending) = pending.take() else {
        return;
    };

    let full_text = pending.text;
    let truncated = truncate_text(&full_text, MAX_TEXT_LEN);
    let text_id = if truncated != full_text {
        let id = format!("t{}", *next_text_id);
        *next_text_id += 1;
        Some(id)
    } else {
        None
    };
    let full_text_pair = text_id.clone().map(|id| (id, full_text.clone()));

    processed.push(ProcessedNode {
        order: pending.order,
        interactive: false,
        element: Element::Text {
            id: text_id,
            text: truncated,
        },
        ref_id: None,
        full_text: full_text_pair,
        full_block: None,
    });
}

fn truncate_processed(mut processed: Vec<ProcessedNode>) -> Vec<ProcessedNode> {
    processed.sort_by_key(|node| node.order);

    let mut selected = Vec::with_capacity(MAX_PAGE_ELEMENTS);
    let mut remaining = Vec::new();

    for node in processed {
        if node.interactive && selected.len() < MAX_PAGE_ELEMENTS {
            selected.push(node);
        } else {
            remaining.push(node);
        }
    }

    for node in remaining {
        if selected.len() >= MAX_PAGE_ELEMENTS {
            break;
        }
        selected.push(node);
    }

    selected.sort_by_key(|node| node.order);
    selected
}

fn extract_element_id(element: &Element) -> String {
    match element {
        Element::Link { id, .. }
        | Element::Button { id, .. }
        | Element::Input { id, .. }
        | Element::Checkbox { id, .. }
        | Element::Radio { id, .. }
        | Element::Select { id, .. }
        | Element::Textarea { id, .. } => id.clone(),
        Element::Text { .. }
        | Element::Heading { .. }
        | Element::List { .. }
        | Element::Table { .. } => String::new(),
    }
}

fn build_list_element(
    items: Vec<String>,
    next_block_id: &mut usize,
) -> (Element, Option<(String, StoredBlock)>) {
    let pages = list_page_ranges(&items);
    if pages.len() <= 1 {
        let total_items = items.len();
        return (
            Element::List {
                id: None,
                items,
                truncated: false,
                shown: total_items,
                total_items,
                current_page: 1,
                total_pages: 1,
            },
            None,
        );
    }

    let block_id = format!("b{}", *next_block_id);
    *next_block_id += 1;
    let (current_page, total_pages, start, end) = first_block_page(&pages);
    let page_items = items[start..end].to_vec();
    (
        Element::List {
            id: Some(block_id.clone()),
            items: page_items,
            truncated: true,
            shown: end - start,
            total_items: items.len(),
            current_page,
            total_pages,
        },
        Some((block_id, StoredBlock::List { items })),
    )
}

fn build_table_element(
    rows: Vec<Vec<String>>,
    next_block_id: &mut usize,
) -> (Element, Option<(String, StoredBlock)>) {
    let pages = table_page_ranges(&rows);
    if pages.len() <= 1 {
        let total_items = rows.len();
        return (
            Element::Table {
                id: None,
                rows,
                truncated: false,
                shown: total_items,
                total_items,
                current_page: 1,
                total_pages: 1,
            },
            None,
        );
    }

    let block_id = format!("b{}", *next_block_id);
    *next_block_id += 1;
    let (current_page, total_pages, start, end) = first_block_page(&pages);
    let page_rows = rows[start..end].to_vec();
    (
        Element::Table {
            id: Some(block_id.clone()),
            rows: page_rows,
            truncated: true,
            shown: end - start,
            total_items: rows.len(),
            current_page,
            total_pages,
        },
        Some((block_id, StoredBlock::Table { rows })),
    )
}

impl StoredBlock {
    fn resolve(&self, block_id: &str, requested_page: Option<u32>) -> BlockData {
        match self {
            Self::List { items } => {
                let pages = list_page_ranges(items);
                let (current_page, total_pages, start, end) =
                    selected_block_page(&pages, requested_page);
                BlockData::List {
                    id: block_id.to_string(),
                    items: items[start..end].to_vec(),
                    truncated: total_pages > 1,
                    shown: end - start,
                    total_items: items.len(),
                    current_page,
                    total_pages,
                }
            }
            Self::Table { rows } => {
                let pages = table_page_ranges(rows);
                let (current_page, total_pages, start, end) =
                    selected_block_page(&pages, requested_page);
                BlockData::Table {
                    id: block_id.to_string(),
                    rows: rows[start..end].to_vec(),
                    truncated: total_pages > 1,
                    shown: end - start,
                    total_items: rows.len(),
                    current_page,
                    total_pages,
                }
            }
        }
    }
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

fn list_page_ranges(items: &[String]) -> Vec<(usize, usize)> {
    paginate_by_render_budget(items.len(), |start, end| list_rendered_lines(end - start))
}

fn table_page_ranges(rows: &[Vec<String>]) -> Vec<(usize, usize)> {
    paginate_by_render_budget(rows.len(), |start, end| {
        2 + rows[start..end]
            .iter()
            .map(|row| rendered_table_row_lines(row, 4))
            .sum::<usize>()
    })
}

fn list_rendered_lines(item_count: usize) -> usize {
    if item_count == 0 { 0 } else { item_count + 2 }
}

fn paginate_by_render_budget(
    total_items: usize,
    rendered_lines_for_range: impl Fn(usize, usize) -> usize,
) -> Vec<(usize, usize)> {
    if total_items == 0 {
        return Vec::new();
    }

    let mut pages = Vec::new();
    let mut start = 0usize;

    while start < total_items {
        let mut end = start;
        while end < total_items {
            let candidate_end = end + 1;
            let lines = rendered_lines_for_range(start, candidate_end);
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

fn element_label(node: &RawNode) -> String {
    let text = normalize_text(&node.text);
    if !text.is_empty() {
        return text;
    }
    node.attrs
        .get("aria-label")
        .or_else(|| node.attrs.get("placeholder"))
        .cloned()
        .unwrap_or_default()
}

fn merge_rect(a: &Rect, b: &Rect) -> Rect {
    let left = a.x.min(b.x);
    let top = a.y.min(b.y);
    let right = (a.x + a.w).max(b.x + b.w);
    let bottom = (a.y + a.h).max(b.y + b.h);
    Rect {
        x: left,
        y: top,
        w: right - left,
        h: bottom - top,
    }
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

fn collect_list_items<'a>(
    container_ref: &str,
    children_by_parent: &HashMap<&'a str, Vec<&'a RawNode>>,
    consumed: &mut HashSet<&'a str>,
) -> Vec<String> {
    let mut items = Vec::new();
    let Some(children) = children_by_parent.get(container_ref) else {
        return items;
    };
    for child in children {
        if child.tag == "li" {
            let text = normalize_text(&child.text);
            if !text.is_empty() {
                items.push(text);
            }
            consumed.insert(child.ref_id.as_str());
            mark_descendants_consumed(&child.ref_id, children_by_parent, consumed);
        }
    }
    items
}

fn collect_table_rows<'a>(
    table_ref: &str,
    children_by_parent: &HashMap<&'a str, Vec<&'a RawNode>>,
    consumed: &mut HashSet<&'a str>,
) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    collect_tr_nodes(table_ref, children_by_parent, consumed, &mut rows);
    rows
}

fn collect_tr_nodes<'a>(
    parent_ref: &str,
    children_by_parent: &HashMap<&'a str, Vec<&'a RawNode>>,
    consumed: &mut HashSet<&'a str>,
    rows: &mut Vec<Vec<String>>,
) {
    let Some(children) = children_by_parent.get(parent_ref) else {
        return;
    };
    for child in children {
        match child.tag.as_str() {
            "tr" => {
                let cells = collect_row_cells(&child.ref_id, children_by_parent);
                if !cells.is_empty() {
                    rows.push(cells);
                }
                consumed.insert(child.ref_id.as_str());
                mark_descendants_consumed(&child.ref_id, children_by_parent, consumed);
            }
            "tbody" | "thead" | "tfoot" => {
                consumed.insert(child.ref_id.as_str());
                collect_tr_nodes(&child.ref_id, children_by_parent, consumed, rows);
            }
            _ => {}
        }
    }
}

fn collect_row_cells<'a>(
    tr_ref: &str,
    children_by_parent: &HashMap<&'a str, Vec<&'a RawNode>>,
) -> Vec<String> {
    let Some(children) = children_by_parent.get(tr_ref) else {
        return Vec::new();
    };
    children
        .iter()
        .filter(|c| matches!(c.tag.as_str(), "td" | "th"))
        .map(|c| normalize_text(&c.text))
        .filter(|t| !t.is_empty())
        .collect()
}

fn mark_descendants_consumed<'a>(
    node_ref: &str,
    children_by_parent: &HashMap<&'a str, Vec<&'a RawNode>>,
    consumed: &mut HashSet<&'a str>,
) {
    let Some(children) = children_by_parent.get(node_ref) else {
        return;
    };
    for child in children {
        consumed.insert(child.ref_id.as_str());
        mark_descendants_consumed(&child.ref_id, children_by_parent, consumed);
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
    fn parse_page_builds_interactive_ids_and_text_ids() {
        let mut link = node("r1", None, "a", "Sign In", 20.0);
        link.attrs.insert("href".into(), "/login".into());
        let text = node(
            "r2",
            None,
            "div",
            "This is a long paragraph that should be truncated because it is far beyond the two hundred character limit used by the CLI rendering layer. The full text should still be available from the hidden text store for the text command.",
            60.0,
        );
        let page = parse_page_from_snapshot(&snapshot(vec![link, text]), Some(1)).unwrap();

        assert_eq!(page.current_page, 1);
        assert_eq!(page.total_pages, 2);
        assert_eq!(page.element_refs.get("e1").map(String::as_str), Some("r1"));
        assert!(page.full_texts.contains_key("t1"));
    }

    #[test]
    fn parse_page_skips_parent_text_when_children_exist() {
        let parent = node("r1", None, "div", "Hello World", 10.0);
        let child = node("r2", Some("r1"), "span", "Hello World", 10.0);
        let page = parse_page_from_snapshot(&snapshot(vec![parent, child]), Some(1)).unwrap();

        assert_eq!(page.elements.len(), 1);
        match &page.elements[0] {
            Element::Text { text, .. } => assert_eq!(text, "Hello World"),
            other => panic!("unexpected element: {other:?}"),
        }
    }

    #[test]
    fn parse_page_does_not_emit_child_text_for_interactive_ancestors() {
        let button = node("r1", None, "button", "Type / to search", 10.0);
        let child = node("r2", Some("r1"), "span", "/", 10.0);
        let page = parse_page_from_snapshot(&snapshot(vec![button, child]), Some(1)).unwrap();

        assert_eq!(page.elements.len(), 1);
        match &page.elements[0] {
            Element::Button { text, .. } => assert_eq!(text, "Type / to search"),
            other => panic!("unexpected element: {other:?}"),
        }
    }

    #[test]
    fn truncate_text_uses_explicit_marker() {
        let input = "a".repeat(MAX_TEXT_LEN + 5);
        let truncated = truncate_text(&input, MAX_TEXT_LEN);
        assert!(truncated.ends_with("[...truncated]"));
    }

    #[test]
    fn parse_page_assigns_block_ids_for_long_lists() {
        let list = node("r1", None, "ul", "", 10.0);
        let mut nodes = vec![list];
        for index in 0..25 {
            nodes.push(node(
                &format!("r{}", index + 2),
                Some("r1"),
                "li",
                &format!("Item {}", index + 1),
                10.0 + index as f64 * 20.0,
            ));
        }

        let page = parse_page_from_snapshot(&snapshot(nodes), Some(1)).unwrap();
        let block_id = match &page.elements[0] {
            Element::List {
                id,
                items,
                truncated,
                shown,
                total_items,
                current_page,
                total_pages,
            } => {
                assert_eq!(id.as_deref(), Some("b1"));
                assert_eq!(items.len(), 18);
                assert!(*truncated);
                assert_eq!(*shown, 18);
                assert_eq!(*total_items, 25);
                assert_eq!(*current_page, 1);
                assert_eq!(*total_pages, 2);
                id.clone().unwrap()
            }
            other => panic!("unexpected element: {other:?}"),
        };

        let block = resolve_block(&page, &block_id, Some(2)).unwrap();
        match block {
            BlockData::List {
                items,
                current_page,
                total_pages,
                shown,
                total_items,
                ..
            } => {
                assert_eq!(current_page, 2);
                assert_eq!(total_pages, 2);
                assert_eq!(shown, 7);
                assert_eq!(total_items, 25);
                assert_eq!(items.first().map(String::as_str), Some("Item 19"));
            }
            other => panic!("unexpected block: {other:?}"),
        }
    }

    #[test]
    fn parse_page_assigns_block_ids_for_long_tables() {
        let table = node("r1", None, "table", "", 10.0);
        let mut nodes = vec![table];
        for index in 0..23 {
            let tr_ref = format!("tr{}", index + 1);
            let td_ref = format!("td{}", index + 1);
            nodes.push(node(
                &tr_ref,
                Some("r1"),
                "tr",
                "",
                10.0 + index as f64 * 20.0,
            ));
            nodes.push(node(
                &td_ref,
                Some(&tr_ref),
                "td",
                &format!("Row {}", index + 1),
                10.0 + index as f64 * 20.0,
            ));
        }

        let page = parse_page_from_snapshot(&snapshot(nodes), Some(1)).unwrap();
        match &page.elements[0] {
            Element::Table {
                id,
                rows,
                truncated,
                shown,
                total_items,
                current_page,
                total_pages,
            } => {
                assert_eq!(id.as_deref(), Some("b1"));
                assert_eq!(rows.len(), 18);
                assert!(*truncated);
                assert_eq!(*shown, 18);
                assert_eq!(*total_items, 23);
                assert_eq!(*current_page, 1);
                assert_eq!(*total_pages, 2);
            }
            other => panic!("unexpected element: {other:?}"),
        }
    }

    #[test]
    fn parse_page_uses_expanded_table_row_lines_for_pagination() {
        let table = node("r1", None, "table", "", 10.0);
        let mut nodes = vec![table];
        for index in 0..7 {
            let tr_ref = format!("tr{}", index + 1);
            let td_ref = format!("td{}", index + 1);
            nodes.push(node(
                &tr_ref,
                Some("r1"),
                "tr",
                "",
                10.0 + index as f64 * 20.0,
            ));
            nodes.push(node(
                &td_ref,
                Some(&tr_ref),
                "td",
                &"x".repeat(200),
                10.0 + index as f64 * 20.0,
            ));
        }

        let page = parse_page_from_snapshot(&snapshot(nodes), Some(1)).unwrap();
        let block_id = match &page.elements[0] {
            Element::Table {
                id,
                rows,
                truncated,
                shown,
                total_items,
                current_page,
                total_pages,
            } => {
                assert_eq!(id.as_deref(), Some("b1"));
                assert_eq!(rows.len(), 6);
                assert!(*truncated);
                assert_eq!(*shown, 6);
                assert_eq!(*total_items, 7);
                assert_eq!(*current_page, 1);
                assert_eq!(*total_pages, 2);
                id.clone().unwrap()
            }
            other => panic!("unexpected element: {other:?}"),
        };

        let block = resolve_block(&page, &block_id, Some(2)).unwrap();
        match block {
            BlockData::Table {
                rows,
                current_page,
                total_pages,
                shown,
                total_items,
                ..
            } => {
                assert_eq!(current_page, 2);
                assert_eq!(total_pages, 2);
                assert_eq!(rows.len(), 1);
                assert_eq!(shown, 1);
                assert_eq!(total_items, 7);
            }
            other => panic!("unexpected block: {other:?}"),
        }
    }

    #[test]
    fn search_snapshot_finds_matches() {
        let snap = snapshot(vec![node(
            "r1",
            None,
            "div",
            "Rust browser automation",
            10.0,
        )]);
        let result = search_snapshot(&snap, "browser");
        assert_eq!(result.matches.len(), 1);
        assert_eq!(result.matches[0].page, 1);
        assert_eq!(result.matches[0].element_id, None);
        assert_eq!(result.matches[0].ref_id, "r1");
    }

    #[test]
    fn search_snapshot_returns_element_ids_for_interactive_matches() {
        let mut button = node("r1", None, "button", "Continue", 10.0);
        button.attrs.insert("aria-label".into(), "Continue".into());
        let result = search_snapshot(&snapshot(vec![button]), "continue");
        assert_eq!(result.matches.len(), 1);
        assert_eq!(result.matches[0].element_id.as_deref(), Some("e1"));
        assert_eq!(result.matches[0].page, 1);
    }

    #[test]
    fn search_snapshot_matches_placeholder_and_href() {
        let mut input = node("r1", None, "input", "", 10.0);
        input
            .attrs
            .insert("placeholder".into(), "Search docs".into());
        input.attrs.insert("type".into(), "text".into());
        let mut link = node("r2", None, "a", "Docs", 40.0);
        link.attrs.insert("href".into(), "/docs/api".into());

        let placeholder_result = search_snapshot(&snapshot(vec![input.clone()]), "docs");
        assert_eq!(placeholder_result.matches.len(), 1);
        assert_eq!(
            placeholder_result.matches[0].element_id.as_deref(),
            Some("e1")
        );

        let href_result = search_snapshot(&snapshot(vec![link]), "api");
        assert_eq!(href_result.matches.len(), 1);
        assert_eq!(href_result.matches[0].element_id.as_deref(), Some("e1"));
    }

    #[test]
    fn search_snapshot_prioritizes_interactive_elements() {
        let text = node("r1", None, "div", "Continue reading", 10.0);
        let button = node("r2", None, "button", "Continue", 40.0);
        let result = search_snapshot(&snapshot(vec![text, button]), "continue");
        assert_eq!(result.matches.len(), 2);
        assert_eq!(result.matches[0].ref_id, "r2");
        assert_eq!(result.matches[0].element_id.as_deref(), Some("e1"));
    }
}
