//! Pure sizing arithmetic for the tag-editor overlay. Holds no ratatui
//! types -- it returns plain data and `ui/overlay.rs` does the styling. Kept
//! `pub` because every test in this crate is an integration test against a
//! public module; there is no in-file `#[cfg(test)]` anywhere.

use crate::complete::MAX_SUGGESTIONS;
use crate::model::TagName;

/// A width-fitted, cursor-aware slice of an agent's applied tags, for the
/// editor's one-line chip row.
pub struct Chips {
    pub visible: Vec<TagName>,
    /// Count dropped by width, rendered as a trailing `+N`. 0 means none.
    pub omitted: usize,
    /// Index into `visible` of the selected chip, re-based after any
    /// dropping. `None` when nothing is selected or `applied` is empty.
    pub cursor: Option<usize>,
}

/// "name ✕" -- the name plus a one-column space and the cross glyph.
fn chip_width(tag: &TagName) -> usize {
    tag.as_str().chars().count() + 2
}

/// Whether `applied[start..start + count]`, plus a trailing marker for
/// whatever is left over, fits in `width` columns. Visible chips are
/// separated by one column; the marker gets one more before it.
fn fits(applied: &[TagName], start: usize, count: usize, width: usize) -> bool {
    let mut used = 0usize;
    for (offset, tag) in applied[start..start + count].iter().enumerate() {
        if offset > 0 {
            used += 1;
        }
        used += chip_width(tag);
    }
    let omitted = applied.len() - count;
    if omitted > 0 {
        if count > 0 {
            used += 1;
        }
        used += 1 + omitted.to_string().len();
    }
    used <= width
}

/// Builds the chip row: front-anchored by default, dropping whole chips off
/// the tail once the row overflows `width`. When `tag_cursor` names a chip
/// that front-anchoring would hide, the window instead slides right just far
/// enough to keep that chip on screen -- dropping leading chips rather than
/// the selected one, since the user is about to remove it.
pub fn chips(applied: &[TagName], tag_cursor: Option<usize>, width: u16) -> Chips {
    if applied.is_empty() {
        return Chips { visible: Vec::new(), omitted: 0, cursor: None };
    }
    let width = width as usize;
    let n = applied.len();

    let default_count = (0..=n).rev().find(|&count| fits(applied, 0, count, width)).unwrap_or(0);

    let (start, count) = match tag_cursor {
        Some(i) if i < n && i >= default_count => {
            let max_count = i + 1;
            let count = (1..=max_count)
                .rev()
                .find(|&count| fits(applied, i + 1 - count, count, width))
                .unwrap_or(1);
            (i + 1 - count, count)
        }
        _ => (0, default_count),
    };

    let visible: Vec<TagName> = applied[start..start + count].to_vec();
    let omitted = n - count;
    let cursor = tag_cursor.and_then(|i| (i >= start && i < start + count).then_some(i - start));

    Chips { visible, omitted, cursor }
}

/// How many suggestion lines fit in a frame of `frame_height`, once the chip
/// row, the add field, the hint, and the two border rows are reserved. Never
/// exceeds `complete::MAX_SUGGESTIONS`; saturates at 0 for a frame too short
/// to hold even the reserved rows.
pub fn suggestion_slots(frame_height: u16) -> usize {
    const RESERVED: u16 = 5; // chip row + add field + hint + 2 borders
    (frame_height.saturating_sub(RESERVED) as usize).min(MAX_SUGGESTIONS)
}
