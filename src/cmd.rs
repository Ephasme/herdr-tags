use crate::herdr;
use crate::model::{FilterState, Mode, SeenAs, TagName, TagStore, DISPLAY_TOKEN, TOKEN_PREFIX};
use crate::reconcile;

/// Resolves which agent an operation targets. `HERDR_PANE_ID` is set for a
/// normal pane invocation; a popup does not get one (plan fact 15), so fall
/// back to the flat context JSON, then to the focused agent.
pub fn focused_pane() -> Result<String, String> {
    if let Ok(pane_id) = std::env::var("HERDR_PANE_ID")
        && !pane_id.is_empty()
    {
        return Ok(pane_id);
    }
    if let Ok(raw) = std::env::var("HERDR_PLUGIN_CONTEXT_JSON")
        && let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw)
        && let Some(pane_id) = value.get("focused_pane_id").and_then(|v| v.as_str())
    {
        return Ok(pane_id.to_string());
    }
    let agents = herdr::list_agents()?;
    agents
        .iter()
        .find(|a| a.focused)
        .map(|a| a.pane_id.clone())
        .ok_or_else(|| "no target pane: pass one explicitly".to_string())
}

/// Reconciles and returns any per-write failures as text, so a partial success
/// is visible without printing from library code.
///
/// **The returned string is empty on full success and otherwise starts with
/// `\n`.** That leading newline lives here rather than at the call sites so
/// every caller can append it unconditionally -- `format!("…{warnings}")` --
/// with no `is_empty` check and no run-on line.
fn reconcile_now(store: &TagStore, filter: &FilterState) -> Result<String, String> {
    let report = reconcile::apply(store, filter)?;
    if report.failures.is_empty() {
        return Ok(String::new());
    }
    Ok(format!(
        "\ntags: {} write(s) failed: {}",
        report.failures.len(),
        report.failures.join("; ")
    ))
}

pub fn add(raw_tag: &str, pane: Option<&str>) -> Result<String, String> {
    let tag = TagName::parse(raw_tag)?;
    let pane_id = match pane {
        Some(p) => p.to_string(),
        None => focused_pane()?,
    };

    let mut store = TagStore::load()?;
    store.add(&pane_id, tag.clone());

    // Record provenance so a human can identify a stale pane-id entry later.
    if let Some(agent) = herdr::list_agents()?.into_iter().find(|a| a.pane_id == pane_id) {
        store.note_seen(
            &pane_id,
            SeenAs {
                workspace_id: Some(agent.workspace_id),
                cwd: agent.cwd,
                agent: agent.agent,
            },
        );
    }
    store.save()?;

    let filter = FilterState::load()?;
    let warnings = reconcile_now(&store, &filter)?;
    Ok(format!("tags: {pane_id} += {}{warnings}", tag.as_str()))
}

pub fn remove(raw_tag: &str, pane: Option<&str>) -> Result<String, String> {
    let tag = TagName::parse(raw_tag)?;
    let pane_id = match pane {
        Some(p) => p.to_string(),
        None => focused_pane()?,
    };

    let mut store = TagStore::load()?;
    store.remove(&pane_id, &tag);
    store.save()?;

    // If that was the tag's last occurrence, it must leave the filter too.
    // Otherwise the view keeps filtering on a `tag_x` token nothing writes any
    // more: an *included* vanished tag matches nothing, so the Agents view goes
    // silently empty with no row to explain why. `delete` has the same rule.
    let mut filter = FilterState::load()?;
    let vanished = !store.all_tags().contains(&tag);
    if vanished {
        filter.forget(&tag);
        filter.save()?;
    }

    let warnings = reconcile_now(&store, &filter)?;
    let mut message = format!("tags: {pane_id} -= {}", tag.as_str());
    if vanished {
        message.push_str(&format!(
            "\ntags: {} was its last agent; dropped from the filter too",
            tag.as_str()
        ));
    }
    message.push_str(&warnings);
    Ok(message)
}

pub fn list() -> Result<String, String> {
    let store = TagStore::load()?;
    let filter = FilterState::load()?;
    let live: Vec<String> = herdr::list_agents()?.into_iter().map(|a| a.pane_id).collect();
    let counts = store.counts(&live);

    let known = store.all_tags();
    if known.is_empty() {
        return Ok("tags: none".to_string());
    }
    let rows: Vec<String> = known
        .iter()
        .map(|tag| {
            let marker = match filter.mode(tag) {
                Mode::In => "+",
                Mode::Out => "-",
                Mode::Off => " ",
            };
            format!(
                "{marker} {:<28} {}",
                tag.as_str(),
                counts.get(tag).copied().unwrap_or(0)
            )
        })
        .collect();
    Ok(rows.join("\n"))
}

pub fn delete(raw_tag: &str) -> Result<String, String> {
    let tag = TagName::parse(raw_tag)?;

    let mut store = TagStore::load()?;
    let touched = store.remove_everywhere(&tag);
    store.save()?;

    // A deleted tag must not linger in the filter, or the view would keep
    // referencing a token nothing writes any more.
    let mut filter = FilterState::load()?;
    filter.forget(&tag);
    filter.save()?;

    let warnings = reconcile_now(&store, &filter)?;
    Ok(format!(
        "tags: deleted {} from {} agent(s){warnings}",
        tag.as_str(),
        touched.len()
    ))
}

pub fn filter(raw_tag: &str, mode: Mode) -> Result<String, String> {
    let tag = TagName::parse(raw_tag)?;
    let mut filter = FilterState::load()?;
    filter.set(tag.clone(), mode);
    filter.save()?;

    let store = TagStore::load()?;
    let warnings = reconcile_now(&store, &filter)?;
    Ok(format!("tags: filter {} -> {mode:?}{warnings}", tag.as_str()))
}

pub fn filter_clear() -> Result<String, String> {
    let mut filter = FilterState::load()?;
    filter.clear();
    filter.save()?;

    let store = TagStore::load()?;
    let warnings = reconcile_now(&store, &filter)?;
    Ok(format!("tags: filter cleared{warnings}"))
}

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

/// Teardown. Tokens have no TTL and the view is owned by this plugin, so both
/// must be removed explicitly before disabling or unlinking it.
///
/// Sweeps `pane.list`, NOT `agent.list`: a pane that carried tags and later
/// stopped hosting a detected agent still holds the tokens, and clearing only
/// current agents would strand them on that pane until the server restarts.
pub fn clear() -> Result<String, String> {
    let panes = herdr::list_panes()?;
    let mut cleared = 0usize;
    let mut touched = 0usize;
    let mut failures: Vec<String> = Vec::new();
    for pane in &panes {
        let mine: Vec<String> = pane
            .tokens
            .keys()
            .filter(|key| key.starts_with(TOKEN_PREFIX) || key.as_str() == DISPLAY_TOKEN)
            .cloned()
            .collect();
        if mine.is_empty() {
            continue;
        }
        touched += 1;
        for key in mine {
            match herdr::set_pane_token(&pane.pane_id, &key, None) {
                Ok(()) => cleared += 1,
                Err(e) => failures.push(e),
            }
        }
    }
    herdr::clear_view()?;
    let mut message = format!(
        "tags: cleared {cleared} token(s) across {touched} pane(s) of {}; agent view released",
        panes.len()
    );
    message.push_str("\ntags: state files are untouched -- `sync` puts everything back");
    if !failures.is_empty() {
        message.push_str(&format!(
            "\ntags: {} clear(s) failed: {}",
            failures.len(),
            failures.join("; ")
        ));
    }
    Ok(message)
}

/// Pane ids are the identity, so an entry for a pane that no longer exists is
/// kept by default (closing a pane must not discard its tags). This drops them
/// deliberately.
pub fn gc() -> Result<String, String> {
    let live: Vec<String> = herdr::list_agents()?.into_iter().map(|a| a.pane_id).collect();
    let mut store = TagStore::load()?;
    let stale = store.stale_panes(&live);
    if stale.is_empty() {
        return Ok("tags: nothing stale".to_string());
    }
    for pane_id in &stale {
        store.panes.remove(pane_id);
        store.seen.remove(pane_id);
    }
    store.save()?;
    Ok(format!(
        "tags: dropped {} stale pane entr(ies): {}",
        stale.len(),
        stale.join(", ")
    ))
}

pub fn paths() -> Result<String, String> {
    Ok(format!(
        "tags:   {}\nfilter: {}",
        TagStore::path()?.display(),
        FilterState::path()?.display()
    ))
}

/// Bindable entry point for the popup: a `[[keys.command]]` can invoke a plugin
/// action but not a plugin pane, so the action re-enters herdr to open one.
/// `HERDR_BIN_PATH` is the portable way to find the running binary.
pub fn open_popup() -> Result<String, String> {
    let bin = std::env::var("HERDR_BIN_PATH").unwrap_or_else(|_| "herdr".to_string());
    let status = std::process::Command::new(&bin)
        .args(["plugin", "pane", "open", "--plugin", "tags", "--entrypoint", "popup"])
        .status()
        .map_err(|e| format!("{bin}: {e}"))?;
    if status.success() {
        return Ok(String::new());
    }
    // `ui_busy` lands here: herdr refuses to open a popup while Settings or
    // Copy mode holds the modal slot (fact 15).
    Err(format!("{bin} plugin pane open failed: {status}"))
}

/// Bindable entry point for the per-agent tag editor: identical to
/// `open_popup` except for `--entrypoint editor`, for the same reason -- a
/// plugin action cannot open its own pane over the socket, so it re-enters
/// herdr through `HERDR_BIN_PATH`.
pub fn open_editor() -> Result<String, String> {
    let bin = std::env::var("HERDR_BIN_PATH").unwrap_or_else(|_| "herdr".to_string());
    let status = std::process::Command::new(&bin)
        .args(["plugin", "pane", "open", "--plugin", "tags", "--entrypoint", "editor"])
        .status()
        .map_err(|e| format!("{bin}: {e}"))?;
    if status.success() {
        return Ok(String::new());
    }
    // `ui_busy` lands here: herdr refuses to open a popup while Settings or
    // Copy mode holds the modal slot (fact 15).
    Err(format!("{bin} plugin pane open failed: {status}"))
}
