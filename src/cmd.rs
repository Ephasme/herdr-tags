use crate::model::{FilterState, TagStore};
use crate::reconcile;

pub fn sync() -> Result<String, String> {
    let store = TagStore::load()?;
    let filter = FilterState::load()?;
    let report = reconcile::apply(&store, &filter)?;
    let summary = format!(
        "tags: {} token write(s), view_active={}",
        report.writes, report.view_active
    );
    if report.failures.is_empty() {
        return Ok(summary);
    }
    // Partial success: report what landed *and* fail, so the plugin log shows
    // both the summary and a non-zero exit.
    Err(format!(
        "{summary}; {} write(s) failed: {}",
        report.failures.len(),
        report.failures.join("; ")
    ))
}

pub fn paths() -> Result<String, String> {
    Ok(format!(
        "tags:   {}\nfilter: {}",
        TagStore::path()?.display(),
        FilterState::path()?.display()
    ))
}
