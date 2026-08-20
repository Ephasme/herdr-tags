use herdr_tags::model::{FilterState, Mode, TagName, TagStore, MAX_TAG_NAME};

#[test]
fn tag_names_are_lowercased_and_trimmed() {
    assert_eq!(TagName::parse("  Review  ").unwrap().as_str(), "review");
    assert_eq!(TagName::parse("WIP").unwrap().as_str(), "wip");
}

#[test]
fn tag_names_reject_what_herdr_would_mangle() {
    assert!(TagName::parse("").is_err());
    assert!(TagName::parse("   ").is_err());
    assert!(TagName::parse("has space").is_err());
    assert!(TagName::parse("dots.not.allowed").is_err());
    assert!(TagName::parse("émoji").is_err());
    assert!(TagName::parse(&"x".repeat(MAX_TAG_NAME + 1)).is_err());
    assert!(TagName::parse(&"x".repeat(MAX_TAG_NAME)).is_ok());
}

#[test]
fn token_key_round_trips() {
    let tag = TagName::parse("review").unwrap();
    assert_eq!(tag.token_key(), "tag_review");
    assert_eq!(TagName::from_token_key("tag_review"), Some(tag));
    assert_eq!(TagName::from_token_key("quota"), None);
    assert_eq!(TagName::from_token_key("folder"), None);
}

#[test]
fn store_adds_removes_and_counts() {
    let mut store = TagStore::default();
    let review = TagName::parse("review").unwrap();
    let wip = TagName::parse("wip").unwrap();

    store.add("w1:p1", review.clone());
    store.add("w1:p1", wip.clone());
    store.add("w5:p1", review.clone());

    assert_eq!(store.tags_for("w1:p1").len(), 2);
    assert_eq!(store.tags_for("nope").len(), 0);

    // Adding twice is idempotent, not a duplicate.
    store.add("w5:p1", review.clone());
    assert_eq!(store.tags_for("w5:p1").len(), 1);

    let counts = store.counts(&["w1:p1".to_string(), "w5:p1".to_string()]);
    assert_eq!(counts.get(&review), Some(&2));
    assert_eq!(counts.get(&wip), Some(&1));

    store.remove("w1:p1", &wip);
    assert_eq!(store.tags_for("w1:p1").len(), 1);
}

#[test]
fn counts_ignore_panes_with_no_live_agent() {
    let mut store = TagStore::default();
    let review = TagName::parse("review").unwrap();
    store.add("w1:p1", review.clone());
    store.add("wGONE:p9", review.clone());

    // Only w1:p1 is live, so the tag counts once even though two entries exist.
    let counts = store.counts(&["w1:p1".to_string()]);
    assert_eq!(counts.get(&review), Some(&1));
}

#[test]
fn remove_everywhere_drops_the_tag_from_every_pane() {
    let mut store = TagStore::default();
    let review = TagName::parse("review").unwrap();
    let wip = TagName::parse("wip").unwrap();
    store.add("w1:p1", review.clone());
    store.add("w5:p1", review.clone());
    store.add("w5:p1", wip.clone());

    let touched = store.remove_everywhere(&review);

    assert_eq!(touched, vec!["w1:p1".to_string(), "w5:p1".to_string()]);
    assert!(store.tags_for("w1:p1").is_empty());
    assert_eq!(store.tags_for("w5:p1").len(), 1);
}

#[test]
fn filter_state_modes_are_mutually_exclusive() {
    let mut filter = FilterState::default();
    let review = TagName::parse("review").unwrap();

    assert_eq!(filter.mode(&review), Mode::Off);
    assert!(filter.is_empty());

    filter.set(review.clone(), Mode::In);
    assert_eq!(filter.mode(&review), Mode::In);
    assert!(!filter.is_empty());

    // Switching to Out must not leave it in both sets.
    filter.set(review.clone(), Mode::Out);
    assert_eq!(filter.mode(&review), Mode::Out);
    assert_eq!(filter.include.len(), 0);
    assert_eq!(filter.exclude.len(), 1);

    filter.set(review.clone(), Mode::Off);
    assert!(filter.is_empty());
}

#[test]
fn deleting_a_tag_also_drops_it_from_the_filter() {
    let mut filter = FilterState::default();
    let review = TagName::parse("review").unwrap();
    filter.set(review.clone(), Mode::In);
    filter.forget(&review);
    assert!(filter.is_empty());
}
