use super::structure::{BlockData, Node, PageData, ViewData};

const MAX_INLINE_XML_LINE_LEN: usize = 100;

fn escape_xml(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '"' => result.push_str("&quot;"),
            '\'' => result.push_str("&apos;"),
            _ => result.push(ch),
        }
    }
    result
}

fn escaped_xml_len(s: &str) -> usize {
    s.chars()
        .map(|ch| match ch {
            '&' => 5,
            '<' => 4,
            '>' => 4,
            '"' => 6,
            '\'' => 6,
            _ => 1,
        })
        .sum()
}

pub fn render_xml(page: &PageData) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "<page url=\"{}\" title=\"{}\" current=\"{}\" total=\"{}\"",
        escape_xml(&page.url),
        escape_xml(&page.title),
        page.current_page,
        page.total_pages,
    ));
    if page.truncated {
        out.push_str(&format!(
            " truncated=\"true\" shown=\"{}\" total_items=\"{}\"",
            page.shown, page.total
        ));
    }
    out.push_str(">\n");
    let top_flags = sibling_strip_flags(&page.nodes);
    for (i, node) in page.nodes.iter().enumerate() {
        render_node(&mut out, node, 2, None, top_flags[i]);
    }
    out.push_str("</page>\n");
    out
}

pub fn render_block_xml(block: &BlockData) -> String {
    let mut out = String::new();
    match block {
        BlockData::List {
            id,
            truncated,
            shown,
            total_items,
            current_page,
            total_pages,
            children,
        } => {
            out.push_str(&format!(
                "<block id=\"{}\" kind=\"list\" current=\"{}\" total=\"{}\"",
                escape_xml(id),
                current_page,
                total_pages,
            ));
            if *truncated {
                out.push_str(&format!(
                    " truncated=\"true\" shown=\"{}\" total_items=\"{}\"",
                    shown, total_items
                ));
            }
            out.push_str(">\n");
            for node in children {
                render_node(&mut out, node, 2, None, false);
            }
            out.push_str("</block>\n");
        }
        BlockData::Table {
            id,
            truncated,
            shown,
            total_items,
            current_page,
            total_pages,
            children,
        } => {
            out.push_str(&format!(
                "<block id=\"{}\" kind=\"table\" current=\"{}\" total=\"{}\"",
                escape_xml(id),
                current_page,
                total_pages,
            ));
            if *truncated {
                out.push_str(&format!(
                    " truncated=\"true\" shown=\"{}\" total_items=\"{}\"",
                    shown, total_items
                ));
            }
            out.push_str(">\n");
            for node in children {
                render_node(&mut out, node, 2, None, false);
            }
            out.push_str("</block>\n");
        }
    }
    out
}

pub fn render_view_xml(view: &ViewData) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "<view target=\"{}\" url=\"{}\" title=\"{}\"",
        escape_xml(&view.target),
        escape_xml(&view.url),
        escape_xml(&view.title),
    ));
    if let Some(context) = &view.context_tag {
        out.push_str(&format!(" context=\"{}\"", escape_xml(context)));
    }
    out.push_str(">\n");
    for node in &view.nodes {
        render_node(&mut out, node, 2, None, false);
    }
    out.push_str("</view>\n");
    out
}

#[derive(Debug, Clone, Default)]
struct SourceMeta {
    roles: Vec<String>,
    classes: Vec<String>,
}

fn render_node(
    out: &mut String,
    node: &Node,
    indent: usize,
    source_meta: Option<SourceMeta>,
    strip_class: bool,
) {
    let indent_str = " ".repeat(indent);
    match node {
        Node::Container {
            tag,
            role,
            class_name,
            children,
        } => {
            if !should_render_container_tag(tag, role.as_deref()) {
                let class_for_meta = if strip_class { None } else { class_name.as_deref() };
                let next_source = extend_source_meta(
                    source_meta.as_ref(),
                    tag,
                    role.as_deref(),
                    class_for_meta,
                );
                let hidden_flags = sibling_strip_flags(children);
                for (i, child) in children.iter().enumerate() {
                    render_node(out, child, indent, Some(next_source.clone()), strip_class || hidden_flags[i]);
                }
                return;
            }
            out.push_str(&format!(
                "{indent_str}<container tag=\"{}\"",
                escape_xml(tag)
            ));
            if strip_class {
                push_semantic_attrs(out, source_meta.as_ref(), role.as_deref(), None);
            } else {
                push_semantic_attrs(
                    out,
                    source_meta.as_ref(),
                    role.as_deref(),
                    class_name.as_deref(),
                );
            }
            if children.is_empty() {
                out.push_str("/>\n");
                return;
            }
            out.push_str(">\n");
            let container_flags = sibling_strip_flags(children);
            for (i, child) in children.iter().enumerate() {
                render_node(out, child, indent + 2, None, strip_class || container_flags[i]);
            }
            out.push_str(&format!("{indent_str}</container>\n"));
        }
        Node::Text { id, text } => {
            out.push_str(&format!("{indent_str}<text"));
            if let Some(id) = id {
                out.push_str(&format!(" id=\"{}\"", escape_xml(id)));
            }
            if !strip_class {
                push_inherited_attrs(out, source_meta.as_ref());
            }
            out.push_str(&format!(">{}</text>\n", escape_xml(text)));
        }
        Node::Heading { level, text } => {
            out.push_str(&format!("{indent_str}<heading level=\"{}\"", level));
            if !strip_class {
                push_inherited_attrs(out, source_meta.as_ref());
            }
            out.push_str(&format!(">{}</heading>\n", escape_xml(text)));
        }
        Node::Link { id, text, href, class_name } => {
            out.push_str(&format!("{indent_str}<link id=\"{}\"", escape_xml(id)));
            if !strip_class {
                push_inherited_and_own_class(out, source_meta.as_ref(), class_name.as_deref());
            }
            if let Some(href) = href {
                out.push_str(&format!(" href=\"{}\"", escape_xml(href)));
            }
            out.push_str(&format!(">{}</link>\n", escape_xml(text)));
        }
        Node::Button { id, text, class_name } => {
            out.push_str(&format!("{indent_str}<button id=\"{}\"", escape_xml(id)));
            if !strip_class {
                push_inherited_and_own_class(out, source_meta.as_ref(), class_name.as_deref());
            }
            out.push_str(&format!(">{}</button>\n", escape_xml(text)));
        }
        Node::Input {
            id,
            input_type,
            placeholder,
            value,
            disabled,
        } => {
            out.push_str(&format!(
                "{indent_str}<input id=\"{}\" type=\"{}\"",
                escape_xml(id),
                escape_xml(input_type),
            ));
            push_inherited_attrs(out, source_meta.as_ref());
            if let Some(placeholder) = placeholder {
                out.push_str(&format!(" placeholder=\"{}\"", escape_xml(placeholder)));
            }
            if let Some(value) = value {
                out.push_str(&format!(" value=\"{}\"", escape_xml(value)));
            }
            if *disabled {
                out.push_str(" disabled=\"true\"");
            }
            out.push_str("/>\n");
        }
        Node::Checkbox { id, text, checked } => {
            out.push_str(&format!("{indent_str}<checkbox id=\"{}\"", escape_xml(id)));
            push_inherited_attrs(out, source_meta.as_ref());
            if *checked {
                out.push_str(" checked=\"true\"");
            }
            if text.is_empty() {
                out.push_str("/>\n");
            } else {
                out.push_str(&format!(">{}</checkbox>\n", escape_xml(text)));
            }
        }
        Node::Radio {
            id,
            text,
            name,
            selected,
        } => {
            out.push_str(&format!("{indent_str}<radio id=\"{}\"", escape_xml(id)));
            push_inherited_attrs(out, source_meta.as_ref());
            if let Some(name) = name {
                out.push_str(&format!(" name=\"{}\"", escape_xml(name)));
            }
            if *selected {
                out.push_str(" selected=\"true\"");
            }
            if text.is_empty() {
                out.push_str("/>\n");
            } else {
                out.push_str(&format!(">{}</radio>\n", escape_xml(text)));
            }
        }
        Node::Select {
            id,
            text,
            selected,
            disabled,
        } => {
            out.push_str(&format!("{indent_str}<select id=\"{}\"", escape_xml(id)));
            push_inherited_attrs(out, source_meta.as_ref());
            if let Some(selected) = selected {
                out.push_str(&format!(" value=\"{}\"", escape_xml(selected)));
            }
            if *disabled {
                out.push_str(" disabled=\"true\"");
            }
            if text.is_empty() {
                out.push_str("/>\n");
            } else {
                out.push_str(&format!(">{}</select>\n", escape_xml(text)));
            }
        }
        Node::Textarea {
            id,
            text,
            placeholder,
            disabled,
        } => {
            out.push_str(&format!("{indent_str}<textarea id=\"{}\"", escape_xml(id)));
            push_inherited_attrs(out, source_meta.as_ref());
            if let Some(placeholder) = placeholder {
                out.push_str(&format!(" placeholder=\"{}\"", escape_xml(placeholder)));
            }
            if *disabled {
                out.push_str(" disabled=\"true\"");
            }
            out.push_str(&format!(">{}</textarea>\n", escape_xml(text)));
        }
        Node::List {
            id,
            truncated,
            shown,
            total_items,
            current_page,
            total_pages,
            children,
        } => {
            out.push_str(&format!("{indent_str}<list"));
            push_inherited_attrs(out, source_meta.as_ref());
            if let Some(id) = id {
                out.push_str(&format!(" id=\"{}\"", escape_xml(id)));
            }
            if *truncated {
                out.push_str(&format!(
                    " truncated=\"true\" shown=\"{}\" total_items=\"{}\" current=\"{}\" total=\"{}\"",
                    shown, total_items, current_page, total_pages
                ));
            }
            if children.is_empty() {
                out.push_str("/>\n");
                return;
            }
            out.push_str(">\n");
            let repetitive = list_has_repetitive_items(children);
            let sibling_flags = sibling_strip_flags(children);
            for (i, child) in children.iter().enumerate() {
                let child_strip =
                    strip_class || sibling_flags[i] || (repetitive && i > 0);
                render_node(out, child, indent + 2, None, child_strip);
            }
            out.push_str(&format!("{indent_str}</list>\n"));
        }
        Node::Item { class_name, children } => {
            let class_attr = if strip_class {
                String::new()
            } else {
                class_name
                    .as_deref()
                    .and_then(normalize_class_name)
                    .map(|c| format!(" class=\"{}\"", escape_xml(&c)))
                    .unwrap_or_default()
            };
            if let Some(text) = inline_text_only(children) {
                out.push_str(&format!(
                    "{indent_str}<item{}>{}</item>\n",
                    class_attr,
                    escape_xml(text)
                ));
                return;
            }
            if let [child] = &children[..]
                && let Some(final_leaf) = collapse_inline_wrapper_leaf(child)
                && item_can_inline_single_child(final_leaf)
                && class_attr.is_empty()
            {
                render_node(out, final_leaf, indent, None, strip_class);
                return;
            }
            out.push_str(&format!("{indent_str}<item{}>\n", class_attr));
            for child in children {
                render_node(out, child, indent + 2, None, strip_class);
            }
            out.push_str(&format!("{indent_str}</item>\n"));
        }
        Node::Table {
            id,
            truncated,
            shown,
            total_items,
            current_page,
            total_pages,
            children,
        } => {
            out.push_str(&format!("{indent_str}<table"));
            push_inherited_attrs(out, source_meta.as_ref());
            if let Some(id) = id {
                out.push_str(&format!(" id=\"{}\"", escape_xml(id)));
            }
            if *truncated {
                out.push_str(&format!(
                    " truncated=\"true\" shown=\"{}\" total_items=\"{}\" current=\"{}\" total=\"{}\"",
                    shown, total_items, current_page, total_pages
                ));
            }
            out.push_str(">\n");
            for child in children {
                render_node(out, child, indent + 2, None, strip_class);
            }
            out.push_str(&format!("{indent_str}</table>\n"));
        }
        Node::Row { children } => render_row(out, children, indent),
        Node::Cell { children } => render_cell(out, children, indent),
        Node::Media {
            id,
            tag,
            media_state,
            current_time,
            duration,
            muted,
            resolution,
        } => {
            out.push_str(&format!(
                "{indent_str}<media id=\"{}\" tag=\"{}\" state=\"{}\" time=\"{}\"",
                escape_xml(id),
                escape_xml(tag),
                escape_xml(media_state),
                current_time,
            ));
            if let Some(dur) = duration {
                out.push_str(&format!(" duration=\"{}\"", dur));
            }
            if *muted {
                out.push_str(" muted=\"true\"");
            }
            if let Some(res) = resolution {
                out.push_str(&format!(" resolution=\"{}\"", escape_xml(res)));
            }
            out.push_str("/>\n");
        }
    }
}

fn render_row(out: &mut String, children: &[Node], indent: usize) {
    let indent_str = " ".repeat(indent);
    if children.len() == 1 && inline_text_len(&children[0], indent) <= MAX_INLINE_XML_LINE_LEN {
        let text = inline_text_only(std::slice::from_ref(&children[0])).expect("single text cell");
        out.push_str(&format!(
            "{indent_str}<row><cell>{}</cell></row>\n",
            escape_xml(text),
        ));
        return;
    }

    out.push_str(&format!("{indent_str}<row>\n"));
    for child in children {
        render_node(out, child, indent + 2, None, false);
    }
    out.push_str(&format!("{indent_str}</row>\n"));
}

fn render_cell(out: &mut String, children: &[Node], indent: usize) {
    let indent_str = " ".repeat(indent);
    if let Some(text) = inline_text_only(children) {
        out.push_str(&format!("{indent_str}<cell>{}</cell>\n", escape_xml(text)));
        return;
    }

    out.push_str(&format!("{indent_str}<cell>\n"));
    for child in children {
        render_node(out, child, indent + 2, None, false);
    }
    out.push_str(&format!("{indent_str}</cell>\n"));
}

fn push_inherited_attrs(out: &mut String, source_meta: Option<&SourceMeta>) {
    let Some(source_meta) = source_meta else {
        return;
    };
    if !source_meta.roles.is_empty() {
        out.push_str(&format!(
            " role=\"{}\"",
            escape_xml(&source_meta.roles.join("/"))
        ));
    }
    if !source_meta.classes.is_empty() {
        let compacted = compact_class_path(&source_meta.classes);
        out.push_str(&format!(
            " class=\"{}\"",
            escape_xml(&compacted.join("/"))
        ));
    }
}

fn push_inherited_and_own_class(
    out: &mut String,
    source_meta: Option<&SourceMeta>,
    own_class: Option<&str>,
) {
    push_semantic_attrs(out, source_meta, None, own_class);
}

fn push_semantic_attrs(
    out: &mut String,
    source_meta: Option<&SourceMeta>,
    own_role: Option<&str>,
    own_class: Option<&str>,
) {
    let mut roles = source_meta
        .map(|meta| meta.roles.clone())
        .unwrap_or_default();
    if let Some(role) = own_role.filter(|role| !role.is_empty()) {
        roles.push(role.to_string());
    }
    if !roles.is_empty() {
        out.push_str(&format!(" role=\"{}\"", escape_xml(&roles.join("/"))));
    }

    let mut classes = source_meta
        .map(|meta| meta.classes.clone())
        .unwrap_or_default();
    if let Some(class_name) = own_class.and_then(normalize_class_name) {
        classes.push(class_name);
    }
    if !classes.is_empty() {
        let compacted = compact_class_path(&classes);
        out.push_str(&format!(" class=\"{}\"", escape_xml(&compacted.join("/"))));
    }
}

fn extend_source_meta(
    current: Option<&SourceMeta>,
    _tag: &str,
    role: Option<&str>,
    class_name: Option<&str>,
) -> SourceMeta {
    let mut next = current.cloned().unwrap_or_default();
    if let Some(role) = role.filter(|role| !role.is_empty()) {
        next.roles.push(role.to_string());
    }
    if let Some(class_name) = class_name.and_then(normalize_class_name) {
        next.classes.push(class_name);
    }
    next
}

fn normalize_class_name(class_name: &str) -> Option<String> {
    let parts: Vec<&str> = class_name
        .split_whitespace()
        .filter(|p| !p.is_empty())
        .collect();
    if parts.is_empty() {
        return None;
    }
    // BEM dedup: if "foo" and "foo--bar" or "foo__bar" coexist, drop "foo".
    let mut keep: Vec<bool> = vec![true; parts.len()];
    for (i, base) in parts.iter().enumerate() {
        for (j, other) in parts.iter().enumerate() {
            if i != j
                && other.len() > base.len()
                && (other.starts_with(&format!("{base}--"))
                    || other.starts_with(&format!("{base}__")))
            {
                keep[i] = false;
                break;
            }
        }
    }
    let deduped: Vec<&str> = parts
        .iter()
        .zip(keep.iter())
        .filter(|(_, k)| **k)
        .map(|(p, _)| *p)
        .collect();
    if deduped.is_empty() {
        None
    } else {
        Some(deduped.join("."))
    }
}

/// Remove path segments whose name is a prefix of the next segment.
/// e.g. ["bili-header", "bili-header__bar"] → ["bili-header__bar"]
fn compact_class_path(segments: &[String]) -> Vec<&str> {
    let mut out: Vec<&str> = Vec::with_capacity(segments.len());
    for (i, seg) in segments.iter().enumerate() {
        if let Some(next) = segments.get(i + 1) {
            if next.starts_with(&format!("{seg}__"))
                || next.starts_with(&format!("{seg}--"))
                || next.starts_with(&format!("{seg}-"))
            {
                continue;
            }
        }
        out.push(seg);
    }
    out
}

/// Check if list children are Items with repetitive internal structure.
/// Returns true when >=2 items share the same child-type fingerprint.
fn list_has_repetitive_items(children: &[Node]) -> bool {
    let fingerprints: Vec<Vec<&str>> = children
        .iter()
        .filter_map(|child| match child {
            Node::Item { children, .. } => {
                Some(children.iter().map(node_type_tag).collect())
            }
            _ => None,
        })
        .collect();

    if fingerprints.len() < 2 {
        return false;
    }

    let first = &fingerprints[0];
    fingerprints[1..].iter().all(|fp| fp == first)
}

fn node_type_tag(node: &Node) -> &str {
    match node {
        Node::Container { .. } => "container",
        Node::Text { .. } => "text",
        Node::Heading { .. } => "heading",
        Node::Link { .. } => "link",
        Node::Button { .. } => "button",
        Node::Input { .. } => "input",
        Node::Checkbox { .. } => "checkbox",
        Node::Radio { .. } => "radio",
        Node::Select { .. } => "select",
        Node::Textarea { .. } => "textarea",
        Node::List { .. } => "list",
        Node::Item { .. } => "item",
        Node::Table { .. } => "table",
        Node::Row { .. } => "row",
        Node::Cell { .. } => "cell",
        Node::Media { .. } => "media",
    }
}

fn node_own_class(node: &Node) -> Option<&str> {
    match node {
        Node::Container { class_name, .. }
        | Node::Link { class_name, .. }
        | Node::Button { class_name, .. }
        | Node::Item { class_name, .. } => class_name.as_deref(),
        _ => None,
    }
}

/// Compute which children should have their class stripped due to sibling dedup.
/// Returns a vec of bools, one per child. `true` = strip class for this child.
/// Only deduplicates nodes that have an own class — classless nodes are never stripped
/// by this mechanism (they have nothing to strip at the sibling level).
fn sibling_strip_flags(children: &[Node]) -> Vec<bool> {
    use std::collections::HashSet;
    let mut seen: HashSet<(&str, &str)> = HashSet::new();
    children
        .iter()
        .map(|child| {
            let Some(class) = node_own_class(child) else {
                return false;
            };
            let key = (node_type_tag(child), class);
            !seen.insert(key) // true if already seen → strip
        })
        .collect()
}

fn inline_text_only(children: &[Node]) -> Option<&str> {
    match children {
        [Node::Text { id: None, text }] => Some(text.as_str()),
        _ => None,
    }
}

fn inline_text_len(cell: &Node, indent: usize) -> usize {
    match cell {
        Node::Cell { children } => match inline_text_only(children) {
            Some(text) => indent + "<row><cell></cell></row>".len() + escaped_xml_len(text),
            None => usize::MAX,
        },
        _ => usize::MAX,
    }
}

fn should_render_container_tag(tag: &str, role: Option<&str>) -> bool {
    role.is_some()
        || matches!(
            tag,
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
        )
}

fn item_can_inline_single_child(node: &Node) -> bool {
    matches!(
        node,
        Node::Text { .. }
            | Node::Heading { .. }
            | Node::Link { .. }
            | Node::Button { .. }
            | Node::Input { .. }
            | Node::Checkbox { .. }
            | Node::Radio { .. }
            | Node::Select { .. }
            | Node::Textarea { .. }
            | Node::Media { .. }
    )
}

fn collapse_inline_wrapper_leaf<'a>(node: &'a Node) -> Option<&'a Node> {
    let mut current = node;
    loop {
        match current {
            Node::Container {
                tag,
                role,
                class_name,
                children,
            } if !should_render_container_tag(tag, role.as_deref())
                && class_name.is_none()
                && children.len() == 1 =>
            {
                current = &children[0];
            }
            _ => return Some(current),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(nodes: Vec<Node>) -> PageData {
        PageData {
            url: "https://example.com".into(),
            title: "Example".into(),
            current_page: 1,
            total_pages: 3,
            truncated: false,
            shown: nodes.len(),
            total: nodes.len(),
            nodes,
            element_refs: Default::default(),
            full_texts: Default::default(),
            full_blocks: Default::default(),
        }
    }

    #[test]
    fn render_xml_renders_recursive_structure() {
        let xml = render_xml(&page(vec![Node::Container {
            tag: "section".into(),
            role: None,
            class_name: None,
            children: vec![
                Node::Heading {
                    level: 1,
                    text: "Hello".into(),
                },
                Node::Link {
                    id: "e1".into(),
                    text: "Docs".into(),
                    href: Some("/docs".into()),
                    class_name: None,
                },
            ],
        }]));

        assert!(xml.contains("<container tag=\"section\">"));
        assert!(xml.contains("<heading level=\"1\">Hello</heading>"));
        assert!(xml.contains("<link id=\"e1\" href=\"/docs\">Docs</link>"));
    }

    #[test]
    fn render_xml_renders_nested_table_links() {
        let xml = render_xml(&page(vec![Node::Table {
            id: None,
            truncated: false,
            shown: 1,
            total_items: 1,
            current_page: 1,
            total_pages: 1,
            children: vec![Node::Row {
                children: vec![
                    Node::Cell {
                        children: vec![Node::Text {
                            id: None,
                            text: "1".into(),
                        }],
                    },
                    Node::Cell {
                        children: vec![Node::Link {
                            id: "e12".into(),
                            text: "Red Desert".into(),
                            href: Some("/app/1".into()),
                            class_name: None,
                        }],
                    },
                ],
            }],
        }]));

        assert!(xml.contains(
            "<cell>\n        <link id=\"e12\" href=\"/app/1\">Red Desert</link>\n      </cell>"
        ));
    }

    #[test]
    fn render_block_xml_supports_tree_blocks() {
        let xml = render_block_xml(&BlockData::List {
            id: "b1".into(),
            truncated: true,
            shown: 2,
            total_items: 20,
            current_page: 2,
            total_pages: 10,
            children: vec![
                Node::Item {
                    class_name: None,
                    children: vec![Node::Text {
                        id: None,
                        text: "Alpha".into(),
                    }],
                },
                Node::Item {
                    class_name: None,
                    children: vec![Node::Text {
                        id: None,
                        text: "Beta".into(),
                    }],
                },
            ],
        });

        assert!(xml.contains("<block id=\"b1\" kind=\"list\" current=\"2\" total=\"10\""));
        assert!(xml.contains("<item>Alpha</item>"));
    }

    #[test]
    fn render_xml_flattens_single_interactive_list_item() {
        let xml = render_xml(&page(vec![Node::List {
            id: None,
            truncated: false,
            shown: 1,
            total_items: 1,
            current_page: 1,
            total_pages: 1,
            children: vec![Node::Item {
                class_name: None,
                children: vec![Node::Link {
                    id: "e1".into(),
                    text: "首页".into(),
                    href: Some("/".into()),
                    class_name: None,
                }],
            }],
        }]));

        assert!(xml.contains("<list>\n    <link id=\"e1\" href=\"/\">首页</link>\n  </list>"));
        assert!(!xml.contains("<item>\n    <link"));
    }

    #[test]
    fn render_xml_skips_presentational_containers() {
        let xml = render_xml(&page(vec![Node::Container {
            tag: "div".into(),
            role: None,
            class_name: None,
            children: vec![Node::Container {
                tag: "i".into(),
                role: None,
                class_name: None,
                children: vec![Node::Link {
                    id: "e1".into(),
                    text: "首页".into(),
                    href: Some("/".into()),
                    class_name: None,
                }],
            }],
        }]));

        assert!(xml.contains("<link id=\"e1\" href=\"/\">首页</link>"));
        assert!(!xml.contains("<container tag=\"div\">"));
        assert!(!xml.contains("<container tag=\"i\">"));
    }

    #[test]
    fn render_xml_keeps_item_group_when_single_child_is_container() {
        let xml = render_xml(&page(vec![Node::List {
            id: None,
            truncated: false,
            shown: 1,
            total_items: 1,
            current_page: 1,
            total_pages: 1,
            children: vec![Node::Item {
                class_name: None,
                children: vec![Node::Container {
                    tag: "div".into(),
                    role: None,
                    class_name: None,
                    children: vec![
                        Node::Link {
                            id: "e1".into(),
                            text: "".into(),
                            href: Some("/video".into()),
                            class_name: None,
                        },
                        Node::Text {
                            id: None,
                            text: "1".into(),
                        },
                        Node::Link {
                            id: "e2".into(),
                            text: "标题".into(),
                            href: Some("/video".into()),
                            class_name: None,
                        },
                    ],
                }],
            }],
        }]));

        assert!(xml.contains("<list>\n    <item>\n      <link id=\"e1\" href=\"/video\"></link>"));
        assert!(xml.contains("<text>1</text>"));
        assert!(xml.contains("<link id=\"e2\" href=\"/video\">标题</link>"));
        assert!(!xml.contains("<container tag=\"div\">"));
    }

    #[test]
    fn render_xml_inherits_class_from_flattened_containers() {
        let xml = render_xml(&page(vec![Node::Container {
            tag: "div".into(),
            role: None,
            class_name: Some("outer".into()),
            children: vec![Node::Container {
                tag: "div".into(),
                role: None,
                class_name: Some("mid wrapper".into()),
                children: vec![Node::Container {
                    tag: "div".into(),
                    role: None,
                    class_name: Some("inner".into()),
                    children: vec![Node::Text {
                        id: None,
                        text: "42".into(),
                    }],
                }],
            }],
        }]));

        assert!(xml.contains("class=\"outer/mid.wrapper/inner\""));
    }

    #[test]
    fn bem_dedup_drops_base_when_modifier_exists() {
        // "nav-tabs__item nav-tabs__item--active" → "nav-tabs__item--active"
        let xml = render_xml(&page(vec![Node::Button {
            id: "e1".into(),
            text: "排行榜".into(),
            class_name: Some("nav-tabs__item nav-tabs__item--active".into()),
        }]));
        assert!(xml.contains("class=\"nav-tabs__item--active\""));
        assert!(!xml.contains("nav-tabs__item.nav-tabs__item--active"));
    }

    #[test]
    fn path_dedup_removes_redundant_parent_segment() {
        // path ["bili-header", "bili-header__bar"] → "bili-header__bar"
        let xml = render_xml(&page(vec![Node::Container {
            tag: "div".into(),
            role: None,
            class_name: Some("bili-header".into()),
            children: vec![Node::Container {
                tag: "div".into(),
                role: None,
                class_name: Some("bili-header__bar".into()),
                children: vec![Node::Text {
                    id: None,
                    text: "test".into(),
                }],
            }],
        }]));
        assert!(xml.contains("class=\"bili-header__bar\""));
        assert!(!xml.contains("bili-header/bili-header__bar"));
    }

    #[test]
    fn render_xml_inlines_item_when_only_wrapper_chain_leads_to_single_leaf() {
        let xml = render_xml(&page(vec![Node::List {
            id: None,
            truncated: false,
            shown: 1,
            total_items: 1,
            current_page: 1,
            total_pages: 1,
            children: vec![Node::Item {
                class_name: None,
                children: vec![Node::Container {
                    tag: "li".into(),
                    role: None,
                    class_name: None,
                    children: vec![Node::Container {
                        tag: "div".into(),
                        role: None,
                        class_name: None,
                        children: vec![Node::Link {
                            id: "e16".into(),
                            text: "投稿".into(),
                            href: Some("/upload".into()),
                            class_name: None,
                        }],
                    }],
                }],
            }],
        }]));

        assert!(
            xml.contains("<list>\n    <link id=\"e16\" href=\"/upload\">投稿</link>\n  </list>")
        );
        assert!(!xml.contains("<item>\n    <link id=\"e16\""));
    }

    #[test]
    fn item_renders_class_name() {
        let xml = render_xml(&page(vec![Node::List {
            id: None,
            truncated: false,
            shown: 2,
            total_items: 2,
            current_page: 1,
            total_pages: 1,
            children: vec![
                Node::Item {
                    class_name: Some("rank-item".into()),
                    children: vec![Node::Text {
                        id: None,
                        text: "A".into(),
                    }],
                },
                Node::Item {
                    class_name: Some("rank-item".into()),
                    children: vec![Node::Text {
                        id: None,
                        text: "B".into(),
                    }],
                },
            ],
        }]));
        // First item shows class
        assert!(xml.contains("<item class=\"rank-item\">A</item>"));
        // Second item class is stripped (repetitive structure)
        assert!(xml.contains("<item>B</item>"));
    }

    #[test]
    fn repetitive_items_strip_child_classes() {
        let xml = render_xml(&page(vec![Node::List {
            id: None,
            truncated: false,
            shown: 2,
            total_items: 2,
            current_page: 1,
            total_pages: 1,
            children: vec![
                Node::Item {
                    class_name: None,
                    children: vec![
                        Node::Link {
                            id: "e1".into(),
                            text: "Title A".into(),
                            href: Some("/a".into()),
                            class_name: Some("info-title".into()),
                        },
                        Node::Text {
                            id: None,
                            text: "100".into(),
                        },
                    ],
                },
                Node::Item {
                    class_name: None,
                    children: vec![
                        Node::Link {
                            id: "e2".into(),
                            text: "Title B".into(),
                            href: Some("/b".into()),
                            class_name: Some("info-title".into()),
                        },
                        Node::Text {
                            id: None,
                            text: "200".into(),
                        },
                    ],
                },
            ],
        }]));
        // First item: children have class
        assert!(xml.contains("class=\"info-title\""));
        // Second item: children don't have class
        assert!(xml.contains("<link id=\"e2\" href=\"/b\">Title B</link>"));
        assert!(!xml.contains("e2\" class="));
    }

    #[test]
    fn sibling_buttons_dedup_common_class() {
        let xml = render_xml(&page(vec![
            Node::Button {
                id: "e1".into(),
                text: "Tab A".into(),
                class_name: Some("nav-item".into()),
            },
            Node::Button {
                id: "e2".into(),
                text: "Tab B".into(),
                class_name: Some("nav-item".into()),
            },
            Node::Button {
                id: "e3".into(),
                text: "Tab C".into(),
                class_name: Some("nav-item active".into()),
            },
        ]));
        // First button: shows class
        assert!(xml.contains("e1\" class=\"nav-item\""));
        // Second button: same class → stripped
        assert!(xml.contains("<button id=\"e2\">Tab B</button>"));
        // Third button: different class → kept
        assert!(xml.contains("e3\" class=\"nav-item.active\""));
    }

    #[test]
    fn non_repetitive_items_keep_all_classes() {
        let xml = render_xml(&page(vec![Node::List {
            id: None,
            truncated: false,
            shown: 2,
            total_items: 2,
            current_page: 1,
            total_pages: 1,
            children: vec![
                Node::Item {
                    class_name: None,
                    children: vec![Node::Link {
                        id: "e1".into(),
                        text: "A".into(),
                        href: Some("/a".into()),
                        class_name: Some("type-a".into()),
                    }],
                },
                Node::Item {
                    class_name: None,
                    children: vec![
                        Node::Link {
                            id: "e2".into(),
                            text: "B".into(),
                            href: Some("/b".into()),
                            class_name: Some("type-b".into()),
                        },
                        Node::Text {
                            id: None,
                            text: "extra".into(),
                        },
                    ],
                },
            ],
        }]));
        // Different structure → both keep class
        assert!(xml.contains("class=\"type-a\""));
        assert!(xml.contains("class=\"type-b\""));
    }

    #[test]
    fn render_xml_media_node_with_all_fields() {
        let xml = render_xml(&page(vec![Node::Media {
            id: "e1".into(),
            tag: "video".into(),
            media_state: "playing".into(),
            current_time: 42,
            duration: Some(120),
            muted: true,
            resolution: Some("1920x1080".into()),
        }]));

        assert!(xml.contains(
            "<media id=\"e1\" tag=\"video\" state=\"playing\" time=\"42\" duration=\"120\" muted=\"true\" resolution=\"1920x1080\"/>"
        ));
    }

    #[test]
    fn render_xml_media_node_minimal_fields() {
        let xml = render_xml(&page(vec![Node::Media {
            id: "e2".into(),
            tag: "audio".into(),
            media_state: "paused".into(),
            current_time: 0,
            duration: None,
            muted: false,
            resolution: None,
        }]));

        assert!(xml.contains(
            "<media id=\"e2\" tag=\"audio\" state=\"paused\" time=\"0\"/>"
        ));
        assert!(!xml.contains("duration="));
        assert!(!xml.contains("muted="));
        assert!(!xml.contains("resolution="));
    }
}
