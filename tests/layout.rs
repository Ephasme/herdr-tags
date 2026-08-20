use herdr_tags::layout::{chips, suggestion_slots};
use herdr_tags::model::TagName;

fn tags(names: &[&str]) -> Vec<TagName> {
    names.iter().map(|n| TagName::parse(n).unwrap()).collect()
}

#[test]
fn suggestion_slots_fits_eight_when_the_frame_has_room() {
    assert_eq!(suggestion_slots(13), 8);
}

#[test]
fn suggestion_slots_fits_two_when_the_frame_is_tighter() {
    assert_eq!(suggestion_slots(7), 2);
}

#[test]
fn suggestion_slots_saturates_at_zero_rather_than_underflowing() {
    assert_eq!(suggestion_slots(3), 0);
    assert_eq!(suggestion_slots(0), 0);
}

#[test]
fn chips_that_fit_are_all_shown_with_nothing_omitted() {
    let applied = tags(&["alpha", "bravo", "charlie", "delta", "echo"]);
    let result = chips(&applied, None, 80);
    assert_eq!(result.visible, applied);
    assert_eq!(result.omitted, 0);
}

#[test]
fn a_narrow_width_drops_whole_chips_from_the_tail() {
    let applied = tags(&["alpha", "bravo", "charlie", "delta", "echo"]);
    let result = chips(&applied, None, 18);
    assert!(!result.visible.is_empty());
    assert!(result.visible.len() < applied.len());
    assert_eq!(result.omitted, applied.len() - result.visible.len());
    // Every visible entry is one of the original whole tags -- never a
    // truncated fragment of a name.
    for tag in &result.visible {
        assert!(applied.contains(tag));
    }
    // Front-anchored: the shown chips are the earliest ones, in order.
    assert_eq!(result.visible, applied[..result.visible.len()]);
}

#[test]
fn a_cursor_past_the_visible_span_pulls_the_window_to_include_it() {
    let applied = tags(&["alpha", "bravo", "charlie", "delta", "echo"]);
    let cursor_index = 4;
    // Confirm the fixture actually forces the window: the front-anchored
    // window at this width does not reach the last chip.
    let front = chips(&applied, None, 15);
    assert!(front.visible.len() <= cursor_index);

    let result = chips(&applied, Some(cursor_index), 15);
    let selected = result.cursor.expect("selection must stay visible");
    assert_eq!(result.visible[selected], applied[cursor_index]);
    assert_eq!(result.omitted, applied.len() - result.visible.len());
}

#[test]
fn empty_applied_yields_an_empty_row() {
    let result = chips(&[], Some(0), 80);
    assert!(result.visible.is_empty());
    assert_eq!(result.omitted, 0);
    assert_eq!(result.cursor, None);
}
