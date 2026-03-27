use super::structure::{Element, PageData};

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

    for element in &page.elements {
        render_element(&mut out, element);
    }

    out.push_str("</page>\n");
    out
}

fn render_element(out: &mut String, element: &Element) {
    match element {
        Element::Text { id, text } => {
            if let Some(id) = id {
                out.push_str(&format!(
                    "  <text id=\"{}\">{}</text>\n",
                    escape_xml(id),
                    escape_xml(text),
                ));
            } else {
                out.push_str(&format!("  <text>{}</text>\n", escape_xml(text)));
            }
        }
        Element::Heading { level, text } => {
            out.push_str(&format!(
                "  <heading level=\"{}\">{}</heading>\n",
                level,
                escape_xml(text),
            ));
        }
        Element::Link { id, text, href } => {
            if let Some(href) = href {
                out.push_str(&format!(
                    "  <link id=\"{}\" href=\"{}\">{}</link>\n",
                    escape_xml(id),
                    escape_xml(href),
                    escape_xml(text),
                ));
            } else {
                out.push_str(&format!(
                    "  <link id=\"{}\">{}</link>\n",
                    escape_xml(id),
                    escape_xml(text),
                ));
            }
        }
        Element::Button { id, text } => {
            out.push_str(&format!(
                "  <button id=\"{}\">{}</button>\n",
                escape_xml(id),
                escape_xml(text),
            ));
        }
        Element::Input {
            id,
            input_type,
            placeholder,
            value,
            disabled,
        } => {
            out.push_str(&format!(
                "  <input id=\"{}\" type=\"{}\"",
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
        Element::Checkbox { id, text, checked } => {
            out.push_str(&format!("  <checkbox id=\"{}\"", escape_xml(id)));
            if *checked {
                out.push_str(" checked=\"true\"");
            }
            if text.is_empty() {
                out.push_str("/>\n");
            } else {
                out.push_str(&format!(">{}</checkbox>\n", escape_xml(text)));
            }
        }
        Element::Radio {
            id,
            text,
            name,
            selected,
        } => {
            out.push_str(&format!("  <radio id=\"{}\"", escape_xml(id)));
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
        Element::Select {
            id,
            text,
            selected,
            disabled,
        } => {
            out.push_str(&format!("  <select id=\"{}\"", escape_xml(id)));
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
        Element::Textarea {
            id,
            text,
            placeholder,
            disabled,
        } => {
            out.push_str(&format!("  <textarea id=\"{}\"", escape_xml(id)));
            if let Some(placeholder) = placeholder {
                out.push_str(&format!(" placeholder=\"{}\"", escape_xml(placeholder)));
            }
            if *disabled {
                out.push_str(" disabled=\"true\"");
            }
            out.push_str(&format!(">{}</textarea>\n", escape_xml(text)));
        }
        Element::List { items } => {
            out.push_str("  <list>\n");
            for item in items {
                out.push_str(&format!("    <item>{}</item>\n", escape_xml(item)));
            }
            out.push_str("  </list>\n");
        }
        Element::Table { rows } => {
            out.push_str("  <table>\n");
            for row in rows {
                out.push_str("    <row>\n");
                for cell in row {
                    out.push_str(&format!("      <cell>{}</cell>\n", escape_xml(cell)));
                }
                out.push_str("    </row>\n");
            }
            out.push_str("  </table>\n");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(elements: Vec<Element>) -> PageData {
        PageData {
            url: "https://example.com".into(),
            title: "Example".into(),
            current_page: 1,
            total_pages: 3,
            truncated: true,
            shown: elements.len(),
            total: 999,
            elements,
            element_refs: Default::default(),
            full_texts: Default::default(),
        }
    }

    #[test]
    fn render_xml_supports_new_interactive_types() {
        let xml = render_xml(&page(vec![
            Element::Link {
                id: "e1".into(),
                text: "Sign In".into(),
                href: Some("/login".into()),
            },
            Element::Checkbox {
                id: "e2".into(),
                text: "Remember me".into(),
                checked: true,
            },
            Element::Textarea {
                id: "e3".into(),
                text: "hello".into(),
                placeholder: Some("message".into()),
                disabled: false,
            },
        ]));

        assert!(xml.contains("<page url=\"https://example.com\" title=\"Example\" current=\"1\""));
        assert!(xml.contains("truncated=\"true\""));
        assert!(xml.contains("<link id=\"e1\" href=\"/login\">Sign In</link>"));
        assert!(xml.contains("<checkbox id=\"e2\" checked=\"true\">Remember me</checkbox>"));
        assert!(xml.contains("<textarea id=\"e3\" placeholder=\"message\">hello</textarea>"));
    }
}
