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
    for node in &page.nodes {
        render_node(&mut out, node, 2);
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
                render_node(&mut out, node, 2);
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
                render_node(&mut out, node, 2);
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
        render_node(&mut out, node, 2);
    }
    out.push_str("</view>\n");
    out
}

fn render_node(out: &mut String, node: &Node, indent: usize) {
    let indent_str = " ".repeat(indent);
    match node {
        Node::Container {
            tag,
            role,
            children,
        } => {
            out.push_str(&format!(
                "{indent_str}<container tag=\"{}\"",
                escape_xml(tag)
            ));
            if let Some(role) = role {
                out.push_str(&format!(" role=\"{}\"", escape_xml(role)));
            }
            if children.is_empty() {
                out.push_str("/>\n");
                return;
            }
            out.push_str(">\n");
            for child in children {
                render_node(out, child, indent + 2);
            }
            out.push_str(&format!("{indent_str}</container>\n"));
        }
        Node::Text { id, text } => {
            if let Some(id) = id {
                out.push_str(&format!(
                    "{indent_str}<text id=\"{}\">{}</text>\n",
                    escape_xml(id),
                    escape_xml(text),
                ));
            } else {
                out.push_str(&format!("{indent_str}<text>{}</text>\n", escape_xml(text)));
            }
        }
        Node::Heading { level, text } => {
            out.push_str(&format!(
                "{indent_str}<heading level=\"{}\">{}</heading>\n",
                level,
                escape_xml(text),
            ));
        }
        Node::Link { id, text, href } => {
            if let Some(href) = href {
                out.push_str(&format!(
                    "{indent_str}<link id=\"{}\" href=\"{}\">{}</link>\n",
                    escape_xml(id),
                    escape_xml(href),
                    escape_xml(text),
                ));
            } else {
                out.push_str(&format!(
                    "{indent_str}<link id=\"{}\">{}</link>\n",
                    escape_xml(id),
                    escape_xml(text),
                ));
            }
        }
        Node::Button { id, text } => {
            out.push_str(&format!(
                "{indent_str}<button id=\"{}\">{}</button>\n",
                escape_xml(id),
                escape_xml(text),
            ));
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
            for child in children {
                render_node(out, child, indent + 2);
            }
            out.push_str(&format!("{indent_str}</list>\n"));
        }
        Node::Item { children } => {
            if let Some(text) = inline_text_only(children) {
                out.push_str(&format!("{indent_str}<item>{}</item>\n", escape_xml(text)));
                return;
            }
            out.push_str(&format!("{indent_str}<item>\n"));
            for child in children {
                render_node(out, child, indent + 2);
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
                render_node(out, child, indent + 2);
            }
            out.push_str(&format!("{indent_str}</table>\n"));
        }
        Node::Row { children } => render_row(out, children, indent),
        Node::Cell { children } => render_cell(out, children, indent),
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
        render_node(out, child, indent + 2);
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
        render_node(out, child, indent + 2);
    }
    out.push_str(&format!("{indent_str}</cell>\n"));
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
            children: vec![
                Node::Heading {
                    level: 1,
                    text: "Hello".into(),
                },
                Node::Link {
                    id: "e1".into(),
                    text: "Docs".into(),
                    href: Some("/docs".into()),
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
                    children: vec![Node::Text {
                        id: None,
                        text: "Alpha".into(),
                    }],
                },
                Node::Item {
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
}
