use herdr_tags::model::{FilterState, Mode, TagName};
use herdr_tags::view::{build_filter, describe};
use serde_json::json;

fn tag(name: &str) -> TagName {
    TagName::parse(name).unwrap()
}

fn state(include: &[&str], exclude: &[&str]) -> FilterState {
    let mut filter = FilterState::default();
    for name in include {
        filter.set(tag(name), Mode::In);
    }
    for name in exclude {
        filter.set(tag(name), Mode::Out);
    }
    filter
}

#[test]
fn no_filter_state_means_no_filter_at_all() {
    assert_eq!(build_filter(&state(&[], &[])), None);
}

#[test]
fn a_single_include_is_an_any_over_one_exists() {
    assert_eq!(
        build_filter(&state(&["review"], &[])).unwrap(),
        json!({"op": "any", "filters": [{"op": "exists", "field": {"token": "tag_review"}}]})
    );
}

#[test]
fn two_includes_are_ored_not_anded() {
    assert_eq!(
        build_filter(&state(&["review", "urgent"], &[])).unwrap(),
        json!({"op": "any", "filters": [
            {"op": "exists", "field": {"token": "tag_review"}},
            {"op": "exists", "field": {"token": "tag_urgent"}}
        ]})
    );
}

#[test]
fn a_single_exclude_is_a_bare_not_exists() {
    assert_eq!(
        build_filter(&state(&[], &["wip"])).unwrap(),
        json!({"op": "not", "filter": {"op": "exists", "field": {"token": "tag_wip"}}})
    );
}

#[test]
fn two_excludes_are_anded() {
    assert_eq!(
        build_filter(&state(&[], &["wip", "muted"])).unwrap(),
        json!({"op": "all", "filters": [
            {"op": "not", "filter": {"op": "exists", "field": {"token": "tag_muted"}}},
            {"op": "not", "filter": {"op": "exists", "field": {"token": "tag_wip"}}}
        ]})
    );
}

#[test]
fn includes_and_excludes_combine_as_all_of_any_includes_and_each_not() {
    assert_eq!(
        build_filter(&state(&["review"], &["wip"])).unwrap(),
        json!({"op": "all", "filters": [
            {"op": "any", "filters": [{"op": "exists", "field": {"token": "tag_review"}}]},
            {"op": "not", "filter": {"op": "exists", "field": {"token": "tag_wip"}}}
        ]})
    );
}

#[test]
fn output_is_deterministic_regardless_of_insertion_order() {
    let forward = build_filter(&state(&["alpha", "beta"], &[]));
    let backward = build_filter(&state(&["beta", "alpha"], &[]));
    assert_eq!(forward, backward);
}

#[test]
fn describe_reads_as_a_human_summary() {
    assert_eq!(describe(&state(&[], &[])), "no filter");
    assert_eq!(describe(&state(&["review"], &["wip"])), "in: review · out: wip");
    assert_eq!(describe(&state(&["a", "b"], &[])), "in: a, b");
}
