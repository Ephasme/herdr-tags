use std::collections::{BTreeMap, BTreeSet};

use herdr_tags::herdr::AgentInfo;
use herdr_tags::model::{TagName, TagStore};
use herdr_tags::reconcile::{display_value, plan_tokens, TokenWrite};

fn tag(name: &str) -> TagName {
    TagName::parse(name).unwrap()
}

fn agent(pane_id: &str, tokens: &[(&str, &str)]) -> AgentInfo {
    AgentInfo {
        pane_id: pane_id.to_string(),
        workspace_id: "w1".to_string(),
        tab_id: "w1:t1".to_string(),
        agent: Some("omp".to_string()),
        agent_status: Some("idle".to_string()),
        cwd: None,
        terminal_title_stripped: None,
        focused: false,
        tokens: tokens
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect::<BTreeMap<String, String>>(),
    }
}

#[test]
fn display_value_joins_sorted_tags() {
    let mut tags = BTreeSet::new();
    tags.insert(tag("wip"));
    tags.insert(tag("review"));
    assert_eq!(display_value(&tags).as_deref(), Some("review wip"));
}

#[test]
fn display_value_is_none_when_there_are_no_tags() {
    assert_eq!(display_value(&BTreeSet::new()), None);
}

#[test]
fn display_value_stays_inside_the_eighty_char_token_limit() {
    let mut tags = BTreeSet::new();
    for i in 0..20 {
        tags.insert(tag(&format!("tag{i:02}-abcdefgh")));
    }
    let value = display_value(&tags).unwrap();
    assert!(value.chars().count() <= 80, "got {} chars: {value}", value.chars().count());
    assert!(value.contains('+'), "expected an overflow marker in {value}");
}

#[test]
fn a_settled_agent_produces_no_writes() {
    let mut store = TagStore::default();
    store.add("w1:p1", tag("review"));
    let agents = vec![agent("w1:p1", &[("tag_review", "1"), ("tags", "review")])];
    assert_eq!(plan_tokens(&agents, &store), Vec::<TokenWrite>::new());
}

#[test]
fn a_new_tag_writes_its_token_and_refreshes_the_display() {
    let mut store = TagStore::default();
    store.add("w1:p1", tag("review"));
    let agents = vec![agent("w1:p1", &[])];
    assert_eq!(
        plan_tokens(&agents, &store),
        vec![
            TokenWrite { pane_id: "w1:p1".into(), key: "tag_review".into(), value: Some("1".into()) },
            TokenWrite { pane_id: "w1:p1".into(), key: "tags".into(), value: Some("review".into()) },
        ]
    );
}

#[test]
fn an_untagged_agent_has_its_stale_tokens_cleared() {
    let store = TagStore::default();
    let agents = vec![agent("w1:p1", &[("tag_review", "1"), ("tags", "review")])];
    assert_eq!(
        plan_tokens(&agents, &store),
        vec![
            TokenWrite { pane_id: "w1:p1".into(), key: "tag_review".into(), value: None },
            TokenWrite { pane_id: "w1:p1".into(), key: "tags".into(), value: None },
        ]
    );
}

#[test]
fn other_sources_tokens_are_never_touched() {
    let mut store = TagStore::default();
    store.add("w1:p1", tag("review"));
    let agents = vec![agent(
        "w1:p1",
        &[("tag_review", "1"), ("tags", "review"), ("quota", "$27"), ("folder", "perso")],
    )];
    assert_eq!(plan_tokens(&agents, &store), Vec::<TokenWrite>::new());
}

#[test]
fn tags_recorded_for_panes_with_no_live_agent_write_nothing() {
    let mut store = TagStore::default();
    store.add("wGONE:p1", tag("review"));
    assert_eq!(plan_tokens(&[], &store), Vec::<TokenWrite>::new());
}
