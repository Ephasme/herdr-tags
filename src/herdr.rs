use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

use serde::Deserialize;
use serde_json::{json, Value};

pub const SOURCE: &str = "plugin:tags";

#[derive(Debug, Clone, Deserialize)]
pub struct AgentInfo {
    pub pane_id: String,
    pub workspace_id: String,
    pub tab_id: String,
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub agent_status: Option<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub terminal_title_stripped: Option<String>,
    #[serde(default)]
    pub focused: bool,
    #[serde(default)]
    pub tokens: BTreeMap<String, String>,
}

/// Just enough of `PaneInfo` to sweep tokens. Deliberately not `AgentInfo`:
/// a pane need not host an agent to be carrying this plugin's tokens.
#[derive(Debug, Clone, Deserialize)]
pub struct PaneRef {
    pub pane_id: String,
    #[serde(default)]
    pub tokens: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WorkspaceInfo {
    pub workspace_id: String,
    pub label: String,
    pub number: u32,
}

pub fn call(method: &str, params: Value) -> Result<Value, String> {
    let path = std::env::var("HERDR_SOCKET_PATH")
        .map_err(|_| format!("{method}: HERDR_SOCKET_PATH is unset; run this through herdr"))?;
    let stream = UnixStream::connect(&path).map_err(|e| format!("{method}: connect {path}: {e}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|e| format!("{method}: set timeout: {e}"))?;

    let request = json!({ "id": format!("tags:{method}"), "method": method, "params": params });
    let mut writer = &stream;
    writer
        .write_all(format!("{request}\n").as_bytes())
        .map_err(|e| format!("{method}: write: {e}"))?;
    writer.flush().map_err(|e| format!("{method}: flush: {e}"))?;

    let mut line = String::new();
    BufReader::new(&stream)
        .read_line(&mut line)
        .map_err(|e| format!("{method}: read: {e}"))?;

    let parsed: Value = serde_json::from_str(line.trim())
        .map_err(|e| format!("{method}: response was not JSON: {e}"))?;
    if let Some(error) = parsed.get("error") {
        let message = error.get("message").and_then(Value::as_str).unwrap_or("unknown error");
        return Err(format!("{method}: {message}"));
    }
    Ok(parsed.get("result").cloned().unwrap_or(Value::Null))
}

pub fn list_agents() -> Result<Vec<AgentInfo>, String> {
    let result = call("agent.list", json!({}))?;
    let agents = result.get("agents").cloned().unwrap_or(Value::Array(Vec::new()));
    serde_json::from_value(agents).map_err(|e| format!("agent.list: unexpected shape: {e}"))
}

/// Every pane, agent or not. `agent.list` returns only panes with a *detected*
/// agent (6 of 15 on this machine), so teardown must sweep panes instead: a
/// pane that carried tags and then stopped being an agent still holds the
/// tokens, and clearing only agents would orphan them.
pub fn list_panes() -> Result<Vec<PaneRef>, String> {
    let result = call("pane.list", json!({}))?;
    let panes = result.get("panes").cloned().unwrap_or(Value::Array(Vec::new()));
    serde_json::from_value(panes).map_err(|e| format!("pane.list: unexpected shape: {e}"))
}

pub fn list_workspaces() -> Result<Vec<WorkspaceInfo>, String> {
    let result = call("workspace.list", json!({}))?;
    let workspaces = result.get("workspaces").cloned().unwrap_or(Value::Array(Vec::new()));
    let mut list: Vec<WorkspaceInfo> =
        serde_json::from_value(workspaces).map_err(|e| format!("workspace.list: unexpected shape: {e}"))?;
    list.sort_by_key(|w| w.number);
    Ok(list)
}

/// `None` clears the token. herdr treats an empty value as a clear anyway
/// (see plan fact 11), so callers must pass `None` rather than `Some("")`.
pub fn set_pane_token(pane_id: &str, key: &str, value: Option<&str>) -> Result<(), String> {
    let token = match value {
        Some(v) => json!({ key: v }),
        None => json!({ key: Value::Null }),
    };
    call(
        "pane.report_metadata",
        json!({ "pane_id": pane_id, "source": SOURCE, "tokens": token }),
    )
    .map(|_| ())
}

/// `sort` is deliberately never sent: omitting it preserves the user's
/// `ui.agent_panel_sort` policy (plan fact 7).
pub fn set_view(filter: Option<Value>, label: Option<&str>) -> Result<Value, String> {
    let mut params = json!({ "source": SOURCE });
    if let Some(filter) = filter {
        params["filter"] = filter;
    }
    if let Some(label) = label {
        params["label"] = Value::String(label.to_string());
    }
    call("agent.view.set", params)
}

pub fn clear_view() -> Result<Value, String> {
    call("agent.view.clear", json!({ "source": SOURCE }))
}
