use crate::page::structure::{PageData, SearchResults};
use crate::protocol::messages::Response;

pub fn format_page(page: &PageData, json: bool) -> String {
    if json {
        serde_json::to_string_pretty(page).unwrap()
    } else {
        crate::page::xml::render_xml(page)
    }
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn format_response(response: &Response, json_mode: bool) -> String {
    if json_mode {
        return serde_json::to_string_pretty(response).unwrap();
    }

    if !response.ok {
        return format!(
            "Error: {}",
            response.error.as_deref().unwrap_or("Unknown error")
        );
    }

    match &response.data {
        Some(data) if data.is_object() => data
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| {
                let value = match v {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                format!("{k}: {value}")
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Some(data) => data.to_string(),
        None => String::new(),
    }
}

pub fn format_search_results(results: &SearchResults, json: bool) -> String {
    if json {
        return serde_json::to_string_pretty(results).unwrap();
    }

    if results.matches.is_empty() {
        return format!("No matches for: {}", results.query);
    }

    results
        .matches
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            let element = item
                .element_id
                .as_deref()
                .map(|id| format!(" {id}"))
                .unwrap_or_default();
            format!(
                "{}. [page {}] [{}]{} {}",
                idx + 1,
                item.page,
                item.tag,
                element,
                item.context
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn format_session_list(data: &serde_json::Value) -> String {
    let sessions = match data
        .as_array()
        .or_else(|| data.get("sessions").and_then(|value| value.as_array()))
    {
        Some(arr) => arr,
        None => return String::from("No sessions"),
    };

    if sessions.is_empty() {
        return String::from("No sessions");
    }

    let mut ids = Vec::new();
    let mut urls = Vec::new();
    let mut titles = Vec::new();

    for s in sessions {
        ids.push(
            s.get("session_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        );
        urls.push(
            s.get("url")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        );
        titles.push(
            s.get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        );
    }

    let id_width = ids.iter().map(String::len).max().unwrap_or(0).max(2);
    let url_width = urls.iter().map(String::len).max().unwrap_or(0).max(3);

    let mut lines = Vec::new();
    lines.push(format!(
        "{:<id_width$}    {:<url_width$}    Title",
        "ID", "URL"
    ));

    for i in 0..ids.len() {
        lines.push(format!(
            "{:<id_width$}    {:<url_width$}    {}",
            ids[i], urls[i], titles[i]
        ));
    }

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::page::structure::{Element, PageData, SearchMatch};
    use crate::protocol::messages::Response;

    fn sample_page() -> PageData {
        PageData {
            url: "https://example.com".into(),
            title: "Example".into(),
            current_page: 1,
            total_pages: 1,
            truncated: false,
            shown: 2,
            total: 2,
            elements: vec![
                Element::Heading {
                    level: 1,
                    text: "Hello".into(),
                },
                Element::Text {
                    id: None,
                    text: "World".into(),
                },
            ],
            element_refs: Default::default(),
            full_texts: Default::default(),
        }
    }

    #[test]
    fn format_page_json_contains_elements() {
        let output = format_page(&sample_page(), true);
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["title"], "Example");
        assert!(parsed["elements"].is_array());
    }

    #[test]
    fn format_response_error_is_plain_text() {
        let response = Response {
            id: "1".into(),
            ok: false,
            data: None,
            error: Some("Not found".into()),
        };
        assert_eq!(format_response(&response, false), "Error: Not found");
    }

    #[test]
    fn format_search_results_plain_text() {
        let results = SearchResults {
            query: "rust".into(),
            matches: vec![SearchMatch {
                page: 1,
                element_id: Some("e2".into()),
                ref_id: "r1".into(),
                tag: "div".into(),
                text: "Rust browser automation".into(),
                context: "Rust browser automation".into(),
            }],
        };
        let output = format_search_results(&results, false);
        assert!(output.contains("[div]"));
        assert!(output.contains("[page 1]"));
        assert!(output.contains("e2"));
    }
}
