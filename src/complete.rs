use crate::model::TagName;

pub const MAX_SUGGESTIONS: usize = 8;

/// Suggestions for the tag-editor add field: tags the buffer's folded prefix
/// matches, minus whatever is already applied to this agent, capped at
/// `MAX_SUGGESTIONS` and in `known` order.
///
/// `TagName::parse` already lowercases and trims, so stored names are
/// canonical; only the raw `buffer` needs folding here.
pub fn suggest(known: &[TagName], applied: &[TagName], buffer: &str) -> Vec<TagName> {
    let folded = buffer.trim().to_ascii_lowercase();
    known
        .iter()
        .filter(|tag| tag.as_str().starts_with(folded.as_str()))
        .filter(|tag| !applied.contains(tag))
        .take(MAX_SUGGESTIONS)
        .cloned()
        .collect()
}
