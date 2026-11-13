//! What a recorded call means on every route: its action under each route's name for it, the
//! question or note it sends, and what it acts on.

use serde_json::Value;

/// A call's action under each route's name for it.
pub(crate) fn action(name: &str) -> &str {
    let razor = name.strip_prefix("mcp__razor__");
    name.strip_prefix("functions.").or(razor).unwrap_or(name)
}

/// The input of a call that asks the person a question: a `message` of type question to the
/// parent or to no one, from a session that is no child.
pub(crate) fn ask(child: bool, name: &str, input: &Value) -> Option<Value> {
    said(name, input).filter(|input| !child && input["type"] == "question")
}

/// A question's options, ignoring values that are not strings.
pub(crate) fn options(input: &Value) -> Vec<String> {
    input["options"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(String::from))
        .collect()
}

/// The input of Razor's `message` note or question to the parent or to no one, under each
/// route's name for it; another tool whose name ends in `message` is not it.
pub(super) fn said(name: &str, input: &Value) -> Option<Value> {
    if action(name) != "message" {
        return None;
    }
    let kind = matches!(input["type"].as_str(), Some("note" | "question"));
    (kind && input["to"].as_str().is_none_or(|to| to == "parent")).then(|| input.clone())
}

/// What a call acts on: its command, file, pattern, path, address or query, else its first text,
/// on one line of at most 120 bytes.
pub(crate) fn target(input: &Value) -> String {
    let named = ["command", "file_path", "pattern", "path", "url", "query"];
    let text = named.iter().find_map(|k| input[k].as_str());
    let text = text.or_else(|| input.as_object()?.values().find_map(|v| v.as_str()));
    let line = text.unwrap_or_default().split_whitespace();
    let line = line.collect::<Vec<_>>().join(" ");
    line[..line.floor_char_boundary(120)].into()
}
