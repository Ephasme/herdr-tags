use herdr_tags::complete::{suggest, MAX_SUGGESTIONS};
use herdr_tags::model::TagName;

fn tags(names: &[&str]) -> Vec<TagName> {
    names.iter().map(|n| TagName::parse(n).unwrap()).collect()
}

#[test]
fn empty_buffer_returns_every_known_tag_minus_applied_in_known_order() {
    let known = tags(&["refactor", "review", "wip"]);
    let applied = tags(&["review"]);
    assert_eq!(suggest(&known, &applied, ""), tags(&["refactor", "wip"]));
}

#[test]
fn prefix_match_returns_exact_hit() {
    let known = tags(&["refactor", "review", "wip"]);
    assert_eq!(suggest(&known, &[], "rev"), tags(&["review"]));
}

#[test]
fn buffer_folding_is_case_insensitive() {
    let known = tags(&["refactor", "review", "wip"]);
    assert_eq!(suggest(&known, &[], "REV"), tags(&["review"]));
}

#[test]
fn applied_tag_never_appears_even_on_exact_prefix_match() {
    let known = tags(&["review"]);
    let applied = tags(&["review"]);
    assert_eq!(suggest(&known, &applied, "review"), Vec::new());
}

#[test]
fn more_than_max_suggestions_truncates_to_exactly_eight() {
    let names: Vec<String> = (0..20).map(|i| format!("tag{i:02}")).collect();
    let known = tags(&names.iter().map(String::as_str).collect::<Vec<_>>());
    assert_eq!(suggest(&known, &[], "tag").len(), MAX_SUGGESTIONS);
}

#[test]
fn no_match_returns_empty_vec() {
    let known = tags(&["refactor", "review", "wip"]);
    assert_eq!(suggest(&known, &[], "zzz"), Vec::new());
}
