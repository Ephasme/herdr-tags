use serde_json::{json, Value};

use crate::model::{FilterState, TagName};

fn exists(tag: &TagName) -> Value {
    json!({ "op": "exists", "field": { "token": tag.token_key() } })
}

/// Builds the `filter` for `agent.view.set`.
///
/// `None` means "no filter at all" -- the caller must issue `agent.view.clear`
/// rather than setting an empty filter, since a view with no filter would still
/// be an active projection owned by this plugin.
///
/// Includes are OR-ed; excludes are AND-NOT and therefore win over includes,
/// because they sit as sibling clauses of the same `all`.
pub fn build_filter(state: &FilterState) -> Option<Value> {
    let mut clauses: Vec<Value> = Vec::new();

    if !state.include.is_empty() {
        clauses.push(json!({
            "op": "any",
            "filters": state.include.iter().map(exists).collect::<Vec<_>>(),
        }));
    }
    for tag in &state.exclude {
        clauses.push(json!({ "op": "not", "filter": exists(tag) }));
    }

    match clauses.len() {
        0 => None,
        1 => Some(clauses.remove(0)),
        _ => Some(json!({ "op": "all", "filters": clauses })),
    }
}

pub fn describe(state: &FilterState) -> String {
    if state.is_empty() {
        return "no filter".to_string();
    }
    let join = |tags: &std::collections::BTreeSet<TagName>| {
        tags.iter().map(TagName::as_str).collect::<Vec<_>>().join(", ")
    };
    let mut parts = Vec::new();
    if !state.include.is_empty() {
        parts.push(format!("in: {}", join(&state.include)));
    }
    if !state.exclude.is_empty() {
        parts.push(format!("out: {}", join(&state.exclude)));
    }
    parts.join(" · ")
}
