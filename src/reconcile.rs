use std::collections::BTreeSet;

use crate::herdr::{self, AgentInfo};
use crate::model::{FilterState, TagName, TagStore, DISPLAY_TOKEN, TOKEN_PREFIX};
use crate::view;

/// herdr truncates a token value at 80 characters (plan fact 11), so the
/// display string is capped here instead -- with a visible `+N` marker, so a
/// truncated list never reads as a complete one.
pub const MAX_DISPLAY_CHARS: usize = 80;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenWrite {
    pub pane_id: String,
    pub key: String,
    pub value: Option<String>,
}

#[derive(Debug, Default)]
pub struct Report {
    pub writes: usize,
    pub view_active: bool,
    pub failures: Vec<String>,
}

pub fn display_value(tags: &BTreeSet<TagName>) -> Option<String> {
    if tags.is_empty() {
        return None;
    }
    let names: Vec<&str> = tags.iter().map(TagName::as_str).collect();
    let full = names.join(" ");
    if full.chars().count() <= MAX_DISPLAY_CHARS {
        return Some(full);
    }

    let mut kept: Vec<&str> = Vec::new();
    for (index, name) in names.iter().enumerate() {
        let remaining = names.len() - index;
        let candidate = {
            let mut parts = kept.clone();
            parts.push(name);
            format!("{} +{}", parts.join(" "), remaining.saturating_sub(1))
        };
        if candidate.chars().count() > MAX_DISPLAY_CHARS {
            break;
        }
        kept.push(name);
    }
    let dropped = names.len() - kept.len();
    if kept.is_empty() {
        // Even one name plus a marker does not fit; report the count alone.
        return Some(format!("+{}", names.len()));
    }
    Some(format!("{} +{dropped}", kept.join(" ")))
}

/// Diff desired tag state against what each live agent's pane currently carries.
/// Only keys this plugin owns (`tag_*` and `tags`) are ever considered, so
/// quota-pace's tokens and any other source's are structurally out of reach.
pub fn plan_tokens(agents: &[AgentInfo], store: &TagStore) -> Vec<TokenWrite> {
    let mut writes = Vec::new();

    for agent in agents {
        let desired = store.tags_for(&agent.pane_id);
        let desired_keys: BTreeSet<String> = desired.iter().map(TagName::token_key).collect();
        let current_keys: BTreeSet<String> = agent
            .tokens
            .keys()
            .filter(|key| key.starts_with(TOKEN_PREFIX))
            .cloned()
            .collect();

        for key in desired_keys.difference(&current_keys) {
            writes.push(TokenWrite {
                pane_id: agent.pane_id.clone(),
                key: key.clone(),
                value: Some("1".to_string()),
            });
        }
        for key in current_keys.difference(&desired_keys) {
            writes.push(TokenWrite {
                pane_id: agent.pane_id.clone(),
                key: key.clone(),
                value: None,
            });
        }

        let desired_display = display_value(&desired);
        let current_display = agent.tokens.get(DISPLAY_TOKEN).cloned();
        if desired_display != current_display {
            writes.push(TokenWrite {
                pane_id: agent.pane_id.clone(),
                key: DISPLAY_TOKEN.to_string(),
                value: desired_display,
            });
        }
    }

    writes
}

/// Applies both halves of the desired state: pane tokens, then the projection.
/// A rejected write is collected rather than fatal, so one poisoned pane cannot
/// stop every pane after it.
pub fn apply(store: &TagStore, filter: &FilterState) -> Result<Report, String> {
    let agents = herdr::list_agents()?;
    let mut report = Report::default();

    for write in plan_tokens(&agents, store) {
        match herdr::set_pane_token(&write.pane_id, &write.key, write.value.as_deref()) {
            Ok(()) => report.writes += 1,
            Err(e) => report.failures.push(e),
        }
    }

    match view::build_filter(filter) {
        Some(built) => {
            let label = view::describe(filter);
            match herdr::set_view(Some(built), Some(&label)) {
                Ok(_) => report.view_active = true,
                Err(e) => report.failures.push(e),
            }
        }
        None => {
            if let Err(e) = herdr::clear_view() {
                report.failures.push(e);
            }
        }
    }

    Ok(report)
}
