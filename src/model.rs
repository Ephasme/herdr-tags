use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub const TOKEN_PREFIX: &str = "tag_";
pub const DISPLAY_TOKEN: &str = "tags";
/// herdr caps a token key at 32 chars (plan fact 11); `tag_` eats four.
pub const MAX_TAG_NAME: usize = 28;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TagName(String);

impl TagName {
    pub fn parse(raw: &str) -> Result<Self, String> {
        let normalized = raw.trim().to_ascii_lowercase();
        if normalized.is_empty() {
            return Err("tag name is empty".to_string());
        }
        // Charset before length, so the length check counts characters and not bytes.
        if !normalized
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return Err(
                "tag name may contain only ASCII letters, digits, underscore, and hyphen".to_string(),
            );
        }
        if normalized.len() > MAX_TAG_NAME {
            return Err(format!("tag name may be at most {MAX_TAG_NAME} characters"));
        }
        Ok(Self(normalized))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn token_key(&self) -> String {
        format!("{TOKEN_PREFIX}{}", self.0)
    }

    pub fn from_token_key(key: &str) -> Option<Self> {
        key.strip_prefix(TOKEN_PREFIX)
            .and_then(|rest| Self::parse(rest).ok())
    }
}

pub fn state_dir() -> Result<PathBuf, String> {
    let raw = std::env::var("HERDR_PLUGIN_STATE_DIR")
        .map_err(|_| "HERDR_PLUGIN_STATE_DIR is unset; run this through herdr".to_string())?;
    if raw.is_empty() {
        return Err("HERDR_PLUGIN_STATE_DIR is empty".to_string());
    }
    Ok(PathBuf::from(raw))
}

fn read_json<T: for<'de> Deserialize<'de> + Default>(path: &PathBuf) -> Result<T, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(T::default()),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    if text.trim().is_empty() {
        return Ok(T::default());
    }
    serde_json::from_str(&text).map_err(|e| format!("{} is not valid JSON: {e}", path.display()))
}

fn write_json<T: Serialize>(path: &PathBuf, value: &T) -> Result<(), String> {
    let text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    // herdr creates the state dir at `plugin link`, but running the binary
    // directly before that would otherwise fail on a missing parent with a
    // bare ENOENT that names the temp file rather than the real cause.
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, format!("{text}\n")).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct RawEntry {
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    seen_as: Option<SeenAs>,
}

/// Non-authoritative provenance, written so a human reading the file can tell
/// what a stale pane-id entry used to be. Never used to re-attach tags.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SeenAs {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct RawTagStore {
    #[serde(default)]
    panes: BTreeMap<String, RawEntry>,
}

#[derive(Debug, Default, Clone)]
pub struct TagStore {
    pub panes: BTreeMap<String, BTreeSet<TagName>>,
    pub seen: BTreeMap<String, SeenAs>,
}

impl TagStore {
    pub fn path() -> Result<PathBuf, String> {
        Ok(state_dir()?.join("tags.json"))
    }

    pub fn load() -> Result<Self, String> {
        let raw: RawTagStore = read_json(&Self::path()?)?;
        let mut store = Self::default();
        for (pane_id, entry) in raw.panes {
            let tags: BTreeSet<TagName> =
                entry.tags.iter().filter_map(|t| TagName::parse(t).ok()).collect();
            if let Some(seen) = entry.seen_as {
                store.seen.insert(pane_id.clone(), seen);
            }
            if !tags.is_empty() {
                store.panes.insert(pane_id, tags);
            }
        }
        Ok(store)
    }

    pub fn save(&self) -> Result<(), String> {
        let mut raw = RawTagStore::default();
        for (pane_id, tags) in &self.panes {
            raw.panes.insert(
                pane_id.clone(),
                RawEntry {
                    tags: tags.iter().map(|t| t.as_str().to_string()).collect(),
                    seen_as: self.seen.get(pane_id).cloned(),
                },
            );
        }
        write_json(&Self::path()?, &raw)
    }

    pub fn tags_for(&self, pane_id: &str) -> BTreeSet<TagName> {
        self.panes.get(pane_id).cloned().unwrap_or_default()
    }

    pub fn add(&mut self, pane_id: &str, tag: TagName) {
        self.panes.entry(pane_id.to_string()).or_default().insert(tag);
    }

    pub fn remove(&mut self, pane_id: &str, tag: &TagName) {
        if let Some(tags) = self.panes.get_mut(pane_id) {
            tags.remove(tag);
            if tags.is_empty() {
                self.panes.remove(pane_id);
            }
        }
    }

    /// Returns the pane ids that actually carried the tag, so the caller knows
    /// exactly which panes need a token cleared.
    pub fn remove_everywhere(&mut self, tag: &TagName) -> Vec<String> {
        let touched: Vec<String> = self
            .panes
            .iter()
            .filter(|(_, tags)| tags.contains(tag))
            .map(|(pane_id, _)| pane_id.clone())
            .collect();
        for pane_id in &touched {
            self.remove(pane_id, tag);
        }
        touched
    }

    pub fn note_seen(&mut self, pane_id: &str, seen: SeenAs) {
        self.seen.insert(pane_id.to_string(), seen);
    }

    /// Counts only panes present in `live`, so a tag on a closed pane does not
    /// inflate the number the Tags view shows.
    pub fn counts(&self, live: &[String]) -> BTreeMap<TagName, usize> {
        let mut counts: BTreeMap<TagName, usize> = BTreeMap::new();
        for pane_id in live {
            for tag in self.tags_for(pane_id) {
                *counts.entry(tag).or_insert(0) += 1;
            }
        }
        counts
    }

    /// Every tag the store knows about, live or not — the Tags view lists these
    /// so a tag whose only agent is closed can still be deleted.
    pub fn all_tags(&self) -> BTreeSet<TagName> {
        self.panes.values().flatten().cloned().collect()
    }

    pub fn stale_panes(&self, live: &[String]) -> Vec<String> {
        self.panes
            .keys()
            .filter(|pane_id| !live.contains(pane_id))
            .cloned()
            .collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Off,
    In,
    Out,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct RawFilterState {
    #[serde(default)]
    include: Vec<String>,
    #[serde(default)]
    exclude: Vec<String>,
}

#[derive(Debug, Default, Clone)]
pub struct FilterState {
    pub include: BTreeSet<TagName>,
    pub exclude: BTreeSet<TagName>,
}

impl FilterState {
    pub fn path() -> Result<PathBuf, String> {
        Ok(state_dir()?.join("filter.json"))
    }

    pub fn load() -> Result<Self, String> {
        let raw: RawFilterState = read_json(&Self::path()?)?;
        Ok(Self {
            include: raw.include.iter().filter_map(|t| TagName::parse(t).ok()).collect(),
            exclude: raw.exclude.iter().filter_map(|t| TagName::parse(t).ok()).collect(),
        })
    }

    pub fn save(&self) -> Result<(), String> {
        let raw = RawFilterState {
            include: self.include.iter().map(|t| t.as_str().to_string()).collect(),
            exclude: self.exclude.iter().map(|t| t.as_str().to_string()).collect(),
        };
        write_json(&Self::path()?, &raw)
    }

    pub fn mode(&self, tag: &TagName) -> Mode {
        if self.include.contains(tag) {
            Mode::In
        } else if self.exclude.contains(tag) {
            Mode::Out
        } else {
            Mode::Off
        }
    }

    pub fn set(&mut self, tag: TagName, mode: Mode) {
        self.include.remove(&tag);
        self.exclude.remove(&tag);
        match mode {
            Mode::In => {
                self.include.insert(tag);
            }
            Mode::Out => {
                self.exclude.insert(tag);
            }
            Mode::Off => {}
        }
    }

    pub fn forget(&mut self, tag: &TagName) {
        self.include.remove(tag);
        self.exclude.remove(tag);
    }

    pub fn clear(&mut self) {
        self.include.clear();
        self.exclude.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.include.is_empty() && self.exclude.is_empty()
    }
}
