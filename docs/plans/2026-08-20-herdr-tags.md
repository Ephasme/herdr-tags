# herdr-tags Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Tag herdr agents, list tags with their agent counts, delete a tag everywhere, and filter tagged agents in or out of the Agents sidebar — driven by a ratatui TUI and a scriptable CLI.

**Architecture:** One Rust binary with two faces. The CLI face mutates a durable state file and reconciles it onto the running server: one metadata token per tag (`tag_<name>`) makes tags *filterable*, and one joined `tags` token makes them *renderable*. The ratatui face is a two-view TUI (Agents / Tags) opened as a herdr plugin pane, popup or docked. Filtering uses `agent.view.set` — herdr's own declarative projection over the built-in Agents view — so hiding rows is native behaviour, not a reimplemented sidebar.

**Tech Stack:** Rust (edition 2021), `ratatui` + `crossterm` for the TUI, `serde` + `serde_json` for state and protocol. Nothing else. Raw NDJSON over the Unix socket at `HERDR_SOCKET_PATH`; herdr 0.8.0.

**Spec:** This document. Requirements are the five the user stated, mapped to mechanisms in the Design section. Every API claim was verified against herdr 0.8.0 on 2026-08-20 — live on this machine or in upstream source — and cited in Verified API Facts.

## Global Constraints

- Plugin id `tags`. Metadata source `plugin:tags` — **required**: herdr rejects a plugin-owned `agent.view.set` whose source is not `plugin:<HERDR_PLUGIN_ID>`, and rejects it outright if the plugin is missing or disabled (fact 6).
- `min_herdr_version = "0.8.0"`. `platforms = ["macos"]` — declared only where it is actually verified; the code is POSIX-portable but nothing here runs on Linux.
- Four dependencies: `ratatui`, `crossterm`, `serde` (derive), `serde_json`. **`crossterm` is pinned to `0.29`** on purpose: `ratatui 0.30`'s default `crossterm` feature pulls `ratatui-crossterm`, whose own default feature is `crossterm_0_29` (`cargo info ratatui-crossterm`). A standalone `crossterm` that resolved to a different major would give two incompatible crossterm crates in one build and a wall of type-mismatch errors on `KeyCode`. Pinning makes that coupling deliberate rather than a coincidence that survives until the next release. `Cargo.lock` is committed.
- `cargo clippy -- -D warnings` and `cargo test` must both pass.
- Tag token keys are `tag_<name>`; the whole key must satisfy `^[A-Za-z0-9_-]{1,32}$` (fact 9), so a tag name is **≤28 chars of `[a-z0-9_-]`**, lowercased on the way in.
- Tag tokens carry **no `ttl_ms`**: a tag is durable config, not a live measurement. The `[[startup]]` hook re-stamps every token and re-applies the saved view. Consequence: tokens outlive a disabled plugin, which is what the `clear` command exists for.
- The active view is **transient and singleton** — one projection server-wide, atomically replaced by each set, gone on server exit (fact 6). `sync` re-applies it from `filter.json`.
- Never hook `pane.focused` (fires constantly). `pane.updated` is not hookable at all, which is what makes token writes structurally unable to feed this plugin's own hooks (fact 11).
- All state under `HERDR_PLUGIN_STATE_DIR`; never inside `HERDR_PLUGIN_ROOT`. That directory is **not** derivable from the config dir — they live under different roots (fact 12).
- Reconcile is diff-driven and idempotent: a settled herd produces zero writes.

## Verified API Facts

Confirmed 2026-08-20 against herdr 0.8.0. Items marked *(probed)* were executed live during planning.

1. **`agent.view.set` is the filtering mechanism.** It "installs one transient declarative projection for the built-in Agents view… It controls the expanded and collapsed sidebar, mobile Agents list, mouse targets, indexed focus, and next/previous Agent navigation. It does not change `agent.list`, notifications, detection, or global attention counts." (`herdr.dev/docs/socket-api/`, Agent view queries.)
2. Params: `{source (required), filter (nullable), sort (array), label (nullable)}`. Filter ops: `all`, `any`, `not`, `eq`, `in`, `exists`. Builtin fields: `status`, `workspace_id`, `tab_id`, `pane_id`, `agent`, `seen`, `state_change_seq`. **`{"token":"name"}` as a field filters plugin-reported pane metadata** — the hook this whole design hangs on.
3. *(probed)* `{"op":"exists","field":{"token":"quota"}}` accepted → `{"type":"agent_view","active":true,"source":"probe-tags","label":"probe"}`.
4. *(probed)* `{"op":"all","filters":[{"op":"not","filter":{"op":"exists","field":{"token":"…"}}}]}` accepted → `active:true`. So negation over a token field works, which is requirement 5's "filter out".
5. *(probed)* `agent.view.clear` with a matching `source` → `{"type":"agent_view","active":false}`.
6. `source` must be `plugin:<HERDR_PLUGIN_ID>` for plugins; herdr rejects plugin-owned sets when that plugin is missing or disabled. A successful set **atomically replaces** the previous view. The view lasts until cleared, replaced, its owning plugin is disabled/unlinked/uninstalled, or the server exits. Docs explicitly prescribe saving the query under `HERDR_PLUGIN_STATE_DIR` and reapplying from `[[startup]]`.
7. **Omitting `sort` preserves `ui.agent_panel_sort`** ("When `sort` is omitted, the existing `ui.agent_panel_sort` policy remains active. A custom sort temporarily replaces that policy without rewriting config."). This machine sets `agent_panel_sort = "spaces"`, so this plugin must always omit `sort` — supplying one would silently override the user's config.
8. *(probed)* `agent.list` → `{"type":"agent_list","agents":[…]}`. Returned 6 agents against 15 panes: only panes with a detected agent appear. Each record carries `tokens`, already holding quota-pace's 5 `quota*` keys plus `folder` from the folders plugin.
9. `AgentInfo` fields: `pane_id`, `workspace_id`, `tab_id`, `terminal_id`, `agent`, `agent_status`, `tokens` (≤32, keys `^[A-Za-z0-9_-]{1,32}$`), `cwd`, `foreground_cwd`, `terminal_title`, `terminal_title_stripped`, `title`, `name`, `display_agent`, `focused`, `revision`, `state_change_seq`, `agent_session`, `state_labels`. **No workspace label** — the TUI must join against `workspace.list` for names.
10. `herdr pane report-metadata <PANE_ID> --source <S> --token NAME=VALUE | --clear-token NAME` sets/clears pane tokens. Equivalent socket method `pane.report_metadata`.
11. Token constraints (upstream `src/app/api_helpers.rs`): ≤16 tokens per request, **≤32 per resource**, key charset ASCII alphanumeric + `_` + `-` (≤32 chars), values trimmed, control characters stripped, truncated at 80 chars, and **a value empty after trimming is treated as a clear**. Metadata writes emit `PaneUpdated`, which is absent from `PLUGIN_HOOK_EVENT_KINDS` — so token writes cannot trigger this plugin's own event hooks.
12. Plugin dirs live under different roots, observed on disk: config `~/.config/herdr/plugins/config/<id>`, state `~/.local/state/herdr/plugins/<id>`. There is no `herdr plugin` subcommand for the state dir — read `HERDR_PLUGIN_STATE_DIR`.
13. Manifest-hookable events (`PLUGIN_HOOK_EVENT_KINDS`, upstream `src/api/schema/events.rs`) include `pane.created`, `pane.closed`, `pane.exited`, `pane.moved`, `pane.agent_detected`, `pane.agent_status_changed`; they exclude `pane.updated`, `pane.output_changed`, `workspace.metadata_updated`, `layout.updated`.
14. Runtime plugin commands run with the plugin directory as cwd, `command` is argv with no shell. `[[startup]]` runs after session restore and on live handoff — **not** on `plugin link` or `plugin enable`.
15. A `placement = "popup"` pane is session-modal, receives all input including Escape, has **no pane id**, and closes when its command exits. `plugin.pane.open` returns `ui_busy` while another herdr modal is open. Popups do not receive `HERDR_WORKSPACE_ID`; the workspace arrives in `HERDR_PLUGIN_CONTEXT_JSON`, which is a **flat** object with a top-level `workspace_id` (learned the hard way building `herdr-folders`).
16. Socket transport is NDJSON over the Unix socket: `{"id":…,"method":…,"params":{}}\n` → `{"id":…,"result":{…}}\n`, errors as `{"error":{"code","message"}}`.
17. `plugin link` does not run `[[build]]`; local authors build their own tree.
18. **There is no `agent.view.get`.** The Agent method surface is `list, get, read, explain, send_keys, prompt, wait, rename, focus, start, view.set, view.clear` — the active projection cannot be read back, so the `set` response is the only machine-checkable confirmation it landed.
19. **ratatui 0.30.2 / crossterm 0.29** (`cargo search`, `cargo info`, 2026-08-20). ratatui 0.30 split into a workspace: its default `crossterm` feature pulls `ratatui-crossterm`, whose own default is `crossterm_0_29` — so a standalone `crossterm@0.29` is the matching pin, and a mismatched major would produce two incompatible crossterm crates in one build.
20. **`frame.area()`** is the current accessor (used throughout the 0.30 docs; the old name was `size()`).
21. **`ratatui::run<F, R>(f: F) -> R`** is generic over the closure's return type, so it accepts a closure returning `Result<(), String>` directly, and yields a `DefaultTerminal` (= `Terminal<CrosstermBackend<Stdout>>`). Critically: "All initialization functions install a panic hook that automatically restores the terminal state before panicking." Hand-rolled `enable_raw_mode` + `EnterAlternateScreen` does **not** — a panic unwinds straight past the teardown calls and leaves the operator's terminal in raw mode on an alternate screen. That is why this plan uses `ratatui::run` and never constructs a `Terminal` by hand. (`docs.rs/ratatui/0.30.2/ratatui/init/`)

### Operator-only, and one honest limitation

**No API exposes the projected agent list.** Fact 1 says the view deliberately does not change `agent.list`, and nothing else returns the post-filter set — the sidebar is its only consumer. So "the rows actually disappeared" is confirmable **only by looking at the sidebar**. This plan verifies everything reachable (the set is accepted, reports `active:true` under `source: plugin:tags`, the filter JSON is exactly as designed, and `agent.list` still returns every agent) and marks the visual confirmation as an operator step. Do not claim requirement 5 works on the strength of an `active:true` response alone.

## Design

### Requirements → mechanism

| # | Requirement | Mechanism |
|---|---|---|
| 1 | Add a tag to an agent | Insert into `tags.json[pane_id]`, then stamp `tag_<name>=1` + refresh the `tags` display token on that pane |
| 2 | Remove a tag from an agent | Remove from `tags.json[pane_id]`, clear that pane's `tag_<name>` token, refresh display |
| 3 | List tags with agent counts | Fold `tags.json` over the live `agent.list`; count only panes that currently host an agent |
| 4 | Delete a tag entirely | Drop the tag from every entry in `tags.json`, clear `tag_<name>` on every pane, drop it from `filter.json` too |
| 5 | Filter a tag in / out of the Agents tab | Compose one `agent.view.set` filter from `filter.json`; `exists` for in, `not exists` for out; empty state → `agent.view.clear` |

### Why two token representations

Filter leaves are `eq`, `in`, `exists` — there is **no substring or set-membership-within-a-value op**. A single joined token (`tags = "review wip"`) is therefore unfilterable per tag: `in ["review"]` compares against the whole `"review wip"` string and fails. So the filterable truth must be **one token per tag**, `tag_review`, tested with `exists`.

But sidebar rows are static config (`[ui.sidebar.agents] rows`), so there is no way to render an unknown-ahead-of-time set of `$tag_*` tokens. Rendering needs **one** token with a joined value: `tags = "review wip"`, rendered as `$tags`.

Both are written by the same source in the same reconcile pass, so they cannot drift. Cost: one extra token out of a 32-per-resource budget already carrying quota-pace's 5 — leaving room for ~25 tags on one agent, far past anything useful.

### Tag names

Lowercased, trimmed, `[a-z0-9_-]` only, ≤28 characters (32 minus `tag_`). Anything else is rejected at the edge with a message naming the rule, rather than being silently mangled — herdr would otherwise trim, strip, truncate, or (for a whitespace-only value) reinterpret the write as a *clear* (fact 11). Lowercasing means `Review` and `review` are the same tag, which is almost certainly what a human means.

### Identity: the pane slot

Tags are keyed on **`pane_id`** (e.g. `w5:p1`) — the user's explicit choice. Consequences, both real:

1. **Restarting an agent in a pane keeps its tags.** This is the win: the tag describes the slot in the herd, not one process.
2. **If pane ids shuffle across a server restart, tags follow the id, not the agent.** herdr derives pane ids from workspace and pane numbering, which is stable while workspaces are restored in order, but nothing guarantees it. Mitigation, not prevention: `tags.json` also records a non-authoritative `seen_as` fingerprint per entry (`workspace_id`, `cwd`, last agent label) purely so a human reading the file can tell what a stale entry used to be. The reconcile never re-attaches on its own.
3. Entries are **not** pruned when a pane disappears — otherwise closing a pane would silently discard its tags. `gc` prunes deliberately, and only entries whose pane is currently absent.

### What is taggable: detected agents only

`agent.list` returns only panes with a **detected** agent — 6 of 15 panes on this machine (fact 8). So a pane running a plain shell is not taggable, which is correct rather than a limitation: the requirement is to tag *agents*, and `agent.view.set` projects the Agents view, so a tag on a non-agent could never affect anything requirement 5 cares about.

Two consequences that follow, and are handled rather than merely noted:

1. **An agent whose detection lapses keeps its tags.** The pane leaves `agent.list`, so `plan_tokens` stops reconciling it and its `tag_*` tokens simply sit there — invisible, because no agent row exists to render them. Its `tags.json` entry survives (pane-slot identity), so when an agent is detected in that pane again the tokens are already correct. Nothing to repair.
2. **Teardown must therefore sweep panes, not agents.** `clear` iterates `pane.list`; iterating `agent.list` would strand tokens on exactly the panes in case 1. This is the one place the two lists must not be used interchangeably.

### Filter composition

`filter.json` holds two disjoint sets. Composition is a pure function:

```
clauses = []
if include is non-empty:  clauses.push( any([ exists(tag_x) for x in include ]) )
for x in exclude:         clauses.push( not(exists(tag_x)) )

0 clauses  -> None            (caller issues agent.view.clear instead)
1 clause   -> that clause
n clauses  -> all(clauses)
```

Includes are OR-ed (the user's choice: "either"). Excludes are always AND-NOT, and win over includes because they sit as sibling clauses inside `all`. `sort` is never sent (fact 7). Sets are ordered, so the emitted JSON is deterministic and exactly assertable in tests.

### State files

Both in `HERDR_PLUGIN_STATE_DIR`, written temp-then-rename so a killed process cannot truncate them:

- `tags.json` — `{"panes": {"w5:p1": {"tags": ["review","wip"], "seen_as": {…}}}}`
- `filter.json` — `{"include": ["review"], "exclude": ["wip"]}`

Malformed JSON is a hard error naming the file; a *wrong-shaped but parseable* value degrades gracefully (unknown keys ignored, invalid tag names dropped). That asymmetry is deliberate: a truncated file must not read as "no tags".

### TUI shape

One binary, `ui` subcommand, two panes in the manifest pointing at it (`popup` and `dock` placements — the user wanted both). Two views, switched with `Tab`/`1`/`2`:

- **Agents** — one row per live agent: status glyph, workspace label, tab/pane, agent label, its tags. `a` add a tag (text prompt, completes against existing tags), `r` remove one of the selected agent's tags, `Enter` focus that agent's pane, `j`/`k`/arrows move.
- **Tags** — one row per known tag: name, agent count, filter mode (`·` off / `+` in / `−` out). `i` toggle include, `o` toggle exclude, `D` delete the tag everywhere (confirm), `c` clear all filter state.

Every mutation writes state, reconciles tokens, re-applies the view, and re-reads `agent.list` — so the TUI never shows a stale count. The footer shows the active filter and a hint that filtering only affects the sidebar, never the list in front of you (fact 1).

### Retiring herdr-folders

The user has moved from folders to tags, so the folders plugin should not keep stamping a `folder` token and owning sidebar rows. That is the **last** task, so tags is proven working before folders goes away, and it is reversible (that repo and its plan stay on disk and on GitHub).

## File Structure

```
~/code/perso/herdr-tags/
  Cargo.toml
  Cargo.lock                  # committed; [[build]] is `cargo build --release`
  herdr-plugin.toml           # build, startup, actions, two panes, events
  README.md
  src/
    main.rs                   # argv dispatch across subcommands
    herdr.rs                  # NDJSON socket client + AgentInfo/WorkspaceInfo records
    model.rs                  # TagName, TagStore, FilterState, state file IO
    view.rs                   # PURE: FilterState -> agent.view.set filter JSON
    reconcile.rs              # PURE token diff + apply (tokens & view)
    cmd.rs                    # add/rm/ls/delete/filter/clear/gc/sync/paths
    ui/mod.rs                 # ratatui app loop, view switching, key handling
    ui/agents.rs              # Agents view rendering
    ui/tags.rs                # Tags view rendering
  tests/
    view.rs                   # filter composition, exact JSON
    model.rs                  # tag name rules, store round-trip, filter state
    reconcile.rs              # token diffing
  docs/plans/2026-08-20-herdr-tags.md
```

`view.rs` and `reconcile.rs` hold no IO on purpose — they are where the interesting decisions live, so they are the parts worth testing without a live herd. `herdr.rs` is transport only.

---

### Task 1: Scaffold, socket transport, live smoke

**Files:**
- Create: `Cargo.toml`, `.gitignore`, `src/main.rs`, `src/herdr.rs`

**Interfaces:**
- Produces: `herdr::call(method, params) -> Result<Value, String>`, `herdr::list_agents() -> Result<Vec<AgentInfo>, String>`, `herdr::list_panes() -> Result<Vec<PaneRef>, String>`, `herdr::list_workspaces() -> Result<Vec<WorkspaceInfo>, String>`, `herdr::set_pane_token(pane_id, key, Option<value>) -> Result<(), String>`, `herdr::set_view(filter: Option<Value>, label: Option<&str>) -> Result<Value, String>`, `herdr::clear_view() -> Result<Value, String>`.

- [ ] **Step 1: Create the crate**

```bash
mkdir -p ~/code/perso/herdr-tags && cd ~/code/perso/herdr-tags
cargo init --name herdr-tags
cargo add ratatui serde_json
cargo add crossterm@0.29
cargo add serde --features derive
```

`cargo init` writes its own `.gitignore` containing `/target`; leave it alone. The `crossterm@0.29` pin is load-bearing — see Global Constraints.

- [ ] **Step 2: Write `src/herdr.rs`**

```rust
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
```

- [ ] **Step 3: Temporary `src/main.rs` smoke entry**

```rust
mod herdr;

fn main() {
    match herdr::list_agents() {
        Ok(agents) => {
            println!("agents: {}", agents.len());
            for agent in &agents {
                println!(
                    "  {} ws={} agent={:?} tokens={}",
                    agent.pane_id,
                    agent.workspace_id,
                    agent.agent,
                    agent.tokens.len()
                );
            }
        }
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}
```

- [ ] **Step 4: Verify against the live server**

Run: `cargo run --quiet`
Expected: `agents: N` with N ≥ 1, one line per agent showing a `w<N>:p<M>` pane id, a non-empty workspace id, and a non-zero token count (quota-pace already stamps 5). If it prints `HERDR_SOCKET_PATH is unset`, you are running outside a herdr pane — run it from one.

- [ ] **Step 5: Verify view set/clear round trips — with a non-plugin source**

`herdr::set_view` hardcodes `source: "plugin:tags"`, and herdr **rejects a `plugin:`-sourced view from a plugin it does not know** (fact 6). The plugin is not linked until Task 4, so testing the real source here is impossible — a genuine ordering constraint, not a caveat. Docs allow it: "Other callers may use their own non-`plugin:` source." So this step proves the *transport* with a probe source, and Task 4 Step 9 proves the real one after linking.

Add a temporary second arm to `main` (delete it after this step). The filter matches every agent — every pane already carries a `quota` token — so nothing visibly disappears:

```rust
    // Deliberately NOT herdr::set_view: that hardcodes the plugin: source,
    // which herdr will not accept until this plugin is linked.
    let set = herdr::call(
        "agent.view.set",
        serde_json::json!({
            "source": "probe-tags",
            "label": "smoke",
            "filter": {"op": "exists", "field": {"token": "quota"}},
        }),
    );
    println!("set: {set:?}");
    println!(
        "clear: {:?}",
        herdr::call("agent.view.clear", serde_json::json!({"source": "probe-tags"}))
    );
```

Run: `cargo run --quiet`
Expected: the set prints `"active": true` with `"source": "probe-tags"`, the clear prints `"active": false`. Leaving a stray active view behind would filter the sidebar with nothing maintaining it, so confirm the clear actually reported `active: false` before moving on.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock .gitignore src/ docs/
git commit -m "feat: herdr socket transport for the tags plugin"
```

---

### Task 2: Tag names, stores, filter state

**Files:**
- Create: `src/model.rs`, `tests/model.rs`
- Modify: `src/main.rs` (add `mod model;`)

**Interfaces:**
- Produces: `TagName` (validated newtype, `as_str`, `token_key`, `from_token_key`), `TagStore` (`tags_for`, `add`, `remove`, `remove_everywhere`, `counts`, `load`, `save`), `FilterState` (`mode`, `set`, `clear`, `is_empty`, `load`, `save`), `Mode` (`Off`/`In`/`Out`), `state_dir()`, `MAX_TAG_NAME`, `DISPLAY_TOKEN`, `TOKEN_PREFIX`.

- [ ] **Step 1: Write the failing tests**

Create `tests/model.rs`:

```rust
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
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --test model`
Expected: compile failure — `herdr_tags::model` does not exist. This also forces the crate to expose a library target, added next.

- [ ] **Step 3: Add a library target**

Integration tests need a lib. Add to `Cargo.toml`:

```toml
[lib]
name = "herdr_tags"
path = "src/lib.rs"

[[bin]]
name = "herdr-tags"
path = "src/main.rs"
```

Create `src/lib.rs`:

```rust
pub mod herdr;
pub mod model;
pub mod reconcile;
pub mod view;
```

`src/main.rs` then uses `herdr_tags::…` instead of `mod` declarations. Create empty `src/reconcile.rs` and `src/view.rs` for now so the lib compiles; Tasks 3 and 4 fill them.

- [ ] **Step 4: Write `src/model.rs`**

```rust
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
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --test model`
Expected: PASS, 8 tests.

- [ ] **Step 6: Lint and commit**

```bash
cargo clippy -- -D warnings
git add Cargo.toml src/lib.rs src/model.rs tests/model.rs
git commit -m "feat: tag names, tag store, and filter state"
```

---

### Task 3: Filter composition — requirement 5's core

**Files:**
- Create: `src/view.rs` (replacing the empty placeholder), `tests/view.rs`

**Interfaces:**
- Consumes: `FilterState`, `TagName` from `model`.
- Produces: `view::build_filter(&FilterState) -> Option<serde_json::Value>`, `view::describe(&FilterState) -> String`.

- [ ] **Step 1: Write the failing tests**

Create `tests/view.rs`:

```rust
use herdr_tags::model::{FilterState, Mode, TagName};
use herdr_tags::view::{build_filter, describe};
use serde_json::json;

fn tag(name: &str) -> TagName {
    TagName::parse(name).unwrap()
}

fn state(include: &[&str], exclude: &[&str]) -> FilterState {
    let mut filter = FilterState::default();
    for name in include {
        filter.set(tag(name), Mode::In);
    }
    for name in exclude {
        filter.set(tag(name), Mode::Out);
    }
    filter
}

#[test]
fn no_filter_state_means_no_filter_at_all() {
    assert_eq!(build_filter(&state(&[], &[])), None);
}

#[test]
fn a_single_include_is_an_any_over_one_exists() {
    assert_eq!(
        build_filter(&state(&["review"], &[])).unwrap(),
        json!({"op": "any", "filters": [{"op": "exists", "field": {"token": "tag_review"}}]})
    );
}

#[test]
fn two_includes_are_ored_not_anded() {
    assert_eq!(
        build_filter(&state(&["review", "urgent"], &[])).unwrap(),
        json!({"op": "any", "filters": [
            {"op": "exists", "field": {"token": "tag_review"}},
            {"op": "exists", "field": {"token": "tag_urgent"}}
        ]})
    );
}

#[test]
fn a_single_exclude_is_a_bare_not_exists() {
    assert_eq!(
        build_filter(&state(&[], &["wip"])).unwrap(),
        json!({"op": "not", "filter": {"op": "exists", "field": {"token": "tag_wip"}}})
    );
}

#[test]
fn two_excludes_are_anded() {
    assert_eq!(
        build_filter(&state(&[], &["wip", "muted"])).unwrap(),
        json!({"op": "all", "filters": [
            {"op": "not", "filter": {"op": "exists", "field": {"token": "tag_muted"}}},
            {"op": "not", "filter": {"op": "exists", "field": {"token": "tag_wip"}}}
        ]})
    );
}

#[test]
fn includes_and_excludes_combine_as_all_of_any_includes_and_each_not() {
    assert_eq!(
        build_filter(&state(&["review"], &["wip"])).unwrap(),
        json!({"op": "all", "filters": [
            {"op": "any", "filters": [{"op": "exists", "field": {"token": "tag_review"}}]},
            {"op": "not", "filter": {"op": "exists", "field": {"token": "tag_wip"}}}
        ]})
    );
}

#[test]
fn output_is_deterministic_regardless_of_insertion_order() {
    let forward = build_filter(&state(&["alpha", "beta"], &[]));
    let backward = build_filter(&state(&["beta", "alpha"], &[]));
    assert_eq!(forward, backward);
}

#[test]
fn describe_reads_as_a_human_summary() {
    assert_eq!(describe(&state(&[], &[])), "no filter");
    assert_eq!(describe(&state(&["review"], &["wip"])), "in: review · out: wip");
    assert_eq!(describe(&state(&["a", "b"], &[])), "in: a, b");
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --test view`
Expected: compile failure — `build_filter` and `describe` are not defined.

- [ ] **Step 3: Write `src/view.rs`**

```rust
use serde_json::{json, Value};

use crate::model::{FilterState, TagName};

fn exists(tag: &TagName) -> Value {
    json!({ "op": "exists", "field": { "token": tag.token_key() } })
}

/// Builds the `filter` for `agent.view.set`.
///
/// `None` means "no filter at all" -- the caller must issue `agent.view.clear`
/// rather than setting an empty filter, since a view with no filter would still
/// be an active projection owned by this plugin.
///
/// Includes are OR-ed; excludes are AND-NOT and therefore win over includes,
/// because they sit as sibling clauses of the same `all`.
pub fn build_filter(state: &FilterState) -> Option<Value> {
    let mut clauses: Vec<Value> = Vec::new();

    if !state.include.is_empty() {
        clauses.push(json!({
            "op": "any",
            "filters": state.include.iter().map(exists).collect::<Vec<_>>(),
        }));
    }
    for tag in &state.exclude {
        clauses.push(json!({ "op": "not", "filter": exists(tag) }));
    }

    match clauses.len() {
        0 => None,
        1 => Some(clauses.remove(0)),
        _ => Some(json!({ "op": "all", "filters": clauses })),
    }
}

pub fn describe(state: &FilterState) -> String {
    if state.is_empty() {
        return "no filter".to_string();
    }
    let join = |tags: &std::collections::BTreeSet<TagName>| {
        tags.iter().map(TagName::as_str).collect::<Vec<_>>().join(", ")
    };
    let mut parts = Vec::new();
    if !state.include.is_empty() {
        parts.push(format!("in: {}", join(&state.include)));
    }
    if !state.exclude.is_empty() {
        parts.push(format!("out: {}", join(&state.exclude)));
    }
    parts.join(" · ")
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --test view`
Expected: PASS, 8 tests.

- [ ] **Step 5: Lint and commit**

```bash
cargo clippy -- -D warnings
git add src/view.rs tests/view.rs
git commit -m "feat: compose agent view filters from tag filter state"
```

---

### Task 4: Token reconcile, `sync`, and the manifest

**Files:**
- Create: `src/reconcile.rs` (replacing the placeholder), `tests/reconcile.rs`, `src/cmd.rs`, `herdr-plugin.toml`
- Modify: `src/main.rs` (argv dispatch), `src/lib.rs` (add `pub mod cmd;`)

**Interfaces:**
- Consumes: `AgentInfo` from `herdr`; `TagStore`, `TagName`, `DISPLAY_TOKEN`, `TOKEN_PREFIX` from `model`; `build_filter` from `view`.
- Produces: `reconcile::display_value(&BTreeSet<TagName>) -> Option<String>`, `reconcile::plan_tokens(&[AgentInfo], &TagStore) -> Vec<TokenWrite>`, `reconcile::apply(&TagStore, &FilterState) -> Result<Report, String>`, `TokenWrite { pane_id, key, value }`, `Report { writes, view_active, failures }`.

- [ ] **Step 1: Write the failing tests**

Create `tests/reconcile.rs`:

```rust
use std::collections::{BTreeMap, BTreeSet};

use herdr_tags::herdr::AgentInfo;
use herdr_tags::model::{TagName, TagStore};
use herdr_tags::reconcile::{display_value, plan_tokens, TokenWrite};

fn tag(name: &str) -> TagName {
    TagName::parse(name).unwrap()
}

fn agent(pane_id: &str, tokens: &[(&str, &str)]) -> AgentInfo {
    AgentInfo {
        pane_id: pane_id.to_string(),
        workspace_id: "w1".to_string(),
        tab_id: "w1:t1".to_string(),
        agent: Some("omp".to_string()),
        agent_status: Some("idle".to_string()),
        cwd: None,
        terminal_title_stripped: None,
        focused: false,
        tokens: tokens
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect::<BTreeMap<String, String>>(),
    }
}

#[test]
fn display_value_joins_sorted_tags() {
    let mut tags = BTreeSet::new();
    tags.insert(tag("wip"));
    tags.insert(tag("review"));
    assert_eq!(display_value(&tags).as_deref(), Some("review wip"));
}

#[test]
fn display_value_is_none_when_there_are_no_tags() {
    assert_eq!(display_value(&BTreeSet::new()), None);
}

#[test]
fn display_value_stays_inside_the_eighty_char_token_limit() {
    let mut tags = BTreeSet::new();
    for i in 0..20 {
        tags.insert(tag(&format!("tag{i:02}-abcdefgh")));
    }
    let value = display_value(&tags).unwrap();
    assert!(value.chars().count() <= 80, "got {} chars: {value}", value.chars().count());
    assert!(value.contains('+'), "expected an overflow marker in {value}");
}

#[test]
fn a_settled_agent_produces_no_writes() {
    let mut store = TagStore::default();
    store.add("w1:p1", tag("review"));
    let agents = vec![agent("w1:p1", &[("tag_review", "1"), ("tags", "review")])];
    assert_eq!(plan_tokens(&agents, &store), Vec::<TokenWrite>::new());
}

#[test]
fn a_new_tag_writes_its_token_and_refreshes_the_display() {
    let mut store = TagStore::default();
    store.add("w1:p1", tag("review"));
    let agents = vec![agent("w1:p1", &[])];
    assert_eq!(
        plan_tokens(&agents, &store),
        vec![
            TokenWrite { pane_id: "w1:p1".into(), key: "tag_review".into(), value: Some("1".into()) },
            TokenWrite { pane_id: "w1:p1".into(), key: "tags".into(), value: Some("review".into()) },
        ]
    );
}

#[test]
fn an_untagged_agent_has_its_stale_tokens_cleared() {
    let store = TagStore::default();
    let agents = vec![agent("w1:p1", &[("tag_review", "1"), ("tags", "review")])];
    assert_eq!(
        plan_tokens(&agents, &store),
        vec![
            TokenWrite { pane_id: "w1:p1".into(), key: "tag_review".into(), value: None },
            TokenWrite { pane_id: "w1:p1".into(), key: "tags".into(), value: None },
        ]
    );
}

#[test]
fn other_sources_tokens_are_never_touched() {
    let mut store = TagStore::default();
    store.add("w1:p1", tag("review"));
    let agents = vec![agent(
        "w1:p1",
        &[("tag_review", "1"), ("tags", "review"), ("quota", "$27"), ("folder", "perso")],
    )];
    assert_eq!(plan_tokens(&agents, &store), Vec::<TokenWrite>::new());
}

#[test]
fn tags_recorded_for_panes_with_no_live_agent_write_nothing() {
    let mut store = TagStore::default();
    store.add("wGONE:p1", tag("review"));
    assert_eq!(plan_tokens(&[], &store), Vec::<TokenWrite>::new());
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --test reconcile`
Expected: compile failure — `reconcile::plan_tokens` and friends are not defined.

- [ ] **Step 3: Write `src/reconcile.rs`**

```rust
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
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test`
Expected: PASS across `model`, `view`, and `reconcile` — 24 tests total.

- [ ] **Step 5: Write `src/cmd.rs` with `sync` and `paths`**

**Contract, established here and held by every later `cmd::` function: they return `Result<String, String>` and print nothing.** The `Ok` payload is the human-readable result, possibly multi-line.

This is load-bearing rather than stylistic. Task 7's TUI calls these same functions from inside ratatui's alternate screen, where a stray `println!` would punch raw text through the rendered frame and corrupt the display until the next redraw. Exactly two places turn a message into output: `main.rs` prints it for the CLI, and the TUI puts it in its footer.

```rust
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
    Err(format!("{summary}; {} write(s) failed: {}", report.failures.len(), report.failures.join("; ")))
}

pub fn paths() -> Result<String, String> {
    Ok(format!(
        "tags:   {}\nfilter: {}",
        TagStore::path()?.display(),
        FilterState::path()?.display()
    ))
}
```

- [ ] **Step 6: Write `src/main.rs` argv dispatch**

Hand-rolled rather than pulling in an argument parser — ten subcommands is a `match`, not a dependency.

```rust
use std::process::ExitCode;

use herdr_tags::cmd;

fn usage() -> &'static str {
    "usage: herdr-tags <sync|paths|ui>"
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = args.first().map(String::as_str).unwrap_or("ui");

    let result = match command {
        "sync" => cmd::sync(),
        "paths" => cmd::paths(),
        "ui" => Err("ui is implemented in Task 7".to_string()),
        other => Err(format!("unknown command {other}\n{}", usage())),
    };

    // The single place a cmd:: message reaches stdout.
    match result {
        Ok(message) => {
            if !message.is_empty() {
                println!("{message}");
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("tags: {e}");
            ExitCode::FAILURE
        }
    }
}
```

- [ ] **Step 7: Write `herdr-plugin.toml`**

```toml
id = "tags"
name = "Tags"
version = "0.1.0"
min_herdr_version = "0.8.0"
description = "Tag herdr agents, and filter the Agents view by tag"
platforms = ["macos"]

[[build]]
command = ["cargo", "build", "--release"]

# Tag tokens carry no TTL and the agent view is transient, so both are lost on
# server exit. This hook is the only thing that puts them back. It does NOT run
# on plugin link or enable, so the first reconcile after linking is manual.
[[startup]]
command = ["./target/release/herdr-tags", "sync"]

[[actions]]
id = "sync"
title = "Resync tags"
contexts = ["workspace"]
command = ["./target/release/herdr-tags", "sync"]

[[actions]]
id = "paths"
title = "Show tags state paths"
contexts = ["workspace"]
command = ["./target/release/herdr-tags", "paths"]

# A fresh pane carries no tokens, so re-stamp when one appears or an agent is
# detected in it. pane.focused is deliberately NOT hooked: it fires constantly.
# pane.updated is not hookable at all, which is why writing tokens here cannot
# feed this plugin its own events.
[[events]]
on = "pane.created"
command = ["./target/release/herdr-tags", "sync"]

[[events]]
on = "pane.agent_detected"
command = ["./target/release/herdr-tags", "sync"]
```

- [ ] **Step 8: Build, link, and verify `sync` on an empty store**

```bash
cargo build --release
herdr plugin link ~/code/perso/herdr-tags
herdr plugin list
herdr plugin action invoke tags.sync
sleep 1 && herdr plugin log list --plugin tags --limit 1
```

Expected: `tags` listed as `local:`, and the log showing `tags: 0 token write(s), view_active=false` with `exit_code: 0`. Zero writes is correct — no tags exist yet. This also proves the release binary launches from the plugin's cwd.

- [ ] **Step 9: Verify the state paths land under the right roots**

Run: `herdr plugin action invoke tags.paths && sleep 1 && herdr plugin log list --plugin tags --limit 1`
Expected: both paths under `~/.local/state/herdr/plugins/tags/`, **not** under `~/.config/herdr/plugins/config/tags/` (fact 12).

- [ ] **Step 10: Sanity-check the view path end of `sync`**

```bash
herdr plugin action invoke tags.sync && sleep 1
herdr plugin log list --plugin tags --limit 1
```

Expected: `exit_code: 0`, `0 token write(s)`, `view_active=false`.

Be precise about what this does **not** prove. With an empty `filter.json`, `sync` takes the `agent.view.clear` branch — and a clear whose source does not own the view is a documented **no-op, not an error** ("A source mismatch leaves the active view unchanged"). So a successful clear says nothing about whether `plugin:tags` is accepted. Only a *set* is rejected for an unknown or disabled plugin, and the first real set happens at **Task 6 Step 2**, which is where the source-acceptance gap left open by Task 1 Step 5 actually closes. If `SOURCE` in `src/herdr.rs` ever drifts from `plugin:` + the manifest `id`, Task 6 Step 2 is the step that catches it.

- [ ] **Step 11: Commit**

```bash
cargo clippy -- -D warnings
git add src/reconcile.rs tests/reconcile.rs src/cmd.rs src/main.rs src/lib.rs herdr-plugin.toml
git commit -m "feat: token reconcile, sync command, and plugin manifest"
```

---

### Task 5: The four tag operations (requirements 1–4)

**Files:**
- Modify: `src/cmd.rs`, `src/main.rs`, `herdr-plugin.toml`

**Interfaces:**
- Produces: `cmd::add(tag, pane)`, `cmd::remove(tag, pane)`, `cmd::list()`, `cmd::delete(tag)`, `cmd::filter(tag, mode)`, `cmd::filter_clear()`, `cmd::clear()`, `cmd::gc()`, and `cmd::focused_pane()` resolving the target pane from plugin context. `cmd::open_popup()` is **not** part of this task — Task 7 Step 6 adds both it and its `"open-popup"` dispatch arm, inserted before the `other =>` catch-all.
- **Contract (established in Task 4 Step 5): every `cmd::` function returns `Result<String, String>` and prints nothing.** The `Ok` payload is the human-readable result — possibly multi-line, as for `list`.

Task 7's TUI calls these same functions from inside ratatui's alternate screen, so a stray `println!` here would corrupt the rendered frame. `main.rs` prints for the CLI; the TUI's footer shows the same string. Nothing under `src/cmd.rs` or `src/ui/` calls `println!`.

- [ ] **Step 1: Add pane resolution and the four operations to `src/cmd.rs`**

```rust
use crate::herdr;
use crate::model::{FilterState, Mode, SeenAs, TagName, TagStore, DISPLAY_TOKEN, TOKEN_PREFIX};
use crate::reconcile;

/// Resolves which agent an operation targets. `HERDR_PANE_ID` is set for a
/// normal pane invocation; a popup does not get one (plan fact 15), so fall
/// back to the flat context JSON, then to the focused agent.
pub fn focused_pane() -> Result<String, String> {
    if let Ok(pane_id) = std::env::var("HERDR_PANE_ID") {
        if !pane_id.is_empty() {
            return Ok(pane_id);
        }
    }
    if let Ok(raw) = std::env::var("HERDR_PLUGIN_CONTEXT_JSON") {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) {
            if let Some(pane_id) = value.get("focused_pane_id").and_then(|v| v.as_str()) {
                return Ok(pane_id.to_string());
            }
        }
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
/// every caller can append it unconditionally — `format!("…{warnings}")` — with
/// no `is_empty` check and no run-on line.
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
        message.push_str(&format!("\ntags: {} clear(s) failed: {}", failures.len(), failures.join("; ")));
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
```

- [ ] **Step 2: Extend the argv dispatch in `src/main.rs`**

```rust
use std::process::ExitCode;

use herdr_tags::cmd;
use herdr_tags::model::Mode;

fn usage() -> &'static str {
    concat!(
        "usage:\n",
        "  herdr-tags add <tag> [pane]\n",
        "  herdr-tags rm <tag> [pane]\n",
        "  herdr-tags ls\n",
        "  herdr-tags delete <tag>\n",
        "  herdr-tags filter <tag> <in|out|off>\n",
        "  herdr-tags filter-clear\n",
        "  herdr-tags sync | clear | gc | paths | ui [--dock]"
    )
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = args.first().map(String::as_str).unwrap_or("ui");
    let arg = |n: usize| args.get(n).map(String::as_str);

    let result = match command {
        "add" => match arg(1) {
            Some(tag) => cmd::add(tag, arg(2)),
            None => Err(usage().to_string()),
        },
        "rm" => match arg(1) {
            Some(tag) => cmd::remove(tag, arg(2)),
            None => Err(usage().to_string()),
        },
        "ls" => cmd::list(),
        "delete" => match arg(1) {
            Some(tag) => cmd::delete(tag),
            None => Err(usage().to_string()),
        },
        "filter" => match (arg(1), arg(2)) {
            (Some(tag), Some("in")) => cmd::filter(tag, Mode::In),
            (Some(tag), Some("out")) => cmd::filter(tag, Mode::Out),
            (Some(tag), Some("off")) => cmd::filter(tag, Mode::Off),
            _ => Err(usage().to_string()),
        },
        "filter-clear" => cmd::filter_clear(),
        "sync" => cmd::sync(),
        "clear" => cmd::clear(),
        "gc" => cmd::gc(),
        "paths" => cmd::paths(),
        // Still a placeholder: `src/ui/` does not exist until Task 7, which
        // replaces this arm. `usage()` already advertises it.
        "ui" => Err("ui is implemented in Task 7".to_string()),
        other => Err(format!("unknown command {other}\n{}", usage())),
    };

    // The single place a `cmd::` message reaches stdout.
    match result {
        Ok(message) => {
            if !message.is_empty() {
                println!("{message}");
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("tags: {e}");
            ExitCode::FAILURE
        }
    }
}
```

`Mode` derives `Debug`, which `cmd::filter` uses when it formats the mode into its returned message.

- [ ] **Step 3: Verify requirement 1 — add a tag**

Pick a real pane id from `herdr agent list`, then:

```bash
cd ~/code/perso/herdr-tags && cargo build --release
PANE=$(herdr agent list | python3 -c 'import json,sys; print(json.load(sys.stdin)["result"]["agents"][0]["pane_id"])')
HERDR_PLUGIN_STATE_DIR=~/.local/state/herdr/plugins/tags ./target/release/herdr-tags add review "$PANE"
herdr agent list | python3 -c '
import json,sys
agents=json.load(sys.stdin)["result"]["agents"]
for a in agents:
    t=a.get("tokens") or {}
    mine={k:v for k,v in t.items() if k.startswith("tag_") or k=="tags"}
    if mine: print(a["pane_id"], mine)'
```

Expected: that pane shows `{"tag_review": "1", "tags": "review"}`. Both representations present — one filterable, one renderable.

- [ ] **Step 4: Verify requirement 3 — list with counts**

```bash
HERDR_PLUGIN_STATE_DIR=~/.local/state/herdr/plugins/tags ./target/release/herdr-tags add review "$(herdr agent list | python3 -c 'import json,sys; print(json.load(sys.stdin)["result"]["agents"][1]["pane_id"])')"
HERDR_PLUGIN_STATE_DIR=~/.local/state/herdr/plugins/tags ./target/release/herdr-tags ls
```

Expected: `  review  2`. Now add a second tag to one agent and re-run: two rows, counts 2 and 1.

- [ ] **Step 5: Verify requirement 2 — remove a tag from one agent**

```bash
HERDR_PLUGIN_STATE_DIR=~/.local/state/herdr/plugins/tags ./target/release/herdr-tags rm review "$PANE"
HERDR_PLUGIN_STATE_DIR=~/.local/state/herdr/plugins/tags ./target/release/herdr-tags ls
```

Expected: `review` count drops by one, and that pane's `tag_review` token is gone while the other agent keeps its own. Confirm with the token dump from Step 3.

- [ ] **Step 6: Verify requirement 4 — delete a tag everywhere**

```bash
HERDR_PLUGIN_STATE_DIR=~/.local/state/herdr/plugins/tags ./target/release/herdr-tags delete review
HERDR_PLUGIN_STATE_DIR=~/.local/state/herdr/plugins/tags ./target/release/herdr-tags ls
```

Expected: `deleted review from N agent(s)`, `review` absent from `ls`, and no `tag_review` token on any pane in the dump. Re-run the dump to confirm.

- [ ] **Step 7: Verify idempotence and failure isolation**

```bash
herdr plugin action invoke tags.sync && sleep 1
herdr plugin action invoke tags.sync && sleep 1
herdr plugin log list --plugin tags --limit 2
```

Expected: the second run reports `0 token write(s)`. If some writes were rejected, `sync` exits non-zero with one stderr line carrying both the summary and the `;`-joined failures — the rest still applied. That is the intended partial-success behaviour, not a crash. The mutating commands (`add`, `remove`, `delete`, `filter`) differ deliberately: a rejected token write there is appended to their **stdout** message and the exit stays 0, because the state file — the thing that survives a restart — was written correctly and the next `sync` will retry the token.

- [ ] **Step 8: Add the remaining actions to `herdr-plugin.toml`**

```toml
[[actions]]
id = "ls"
title = "List tags"
contexts = ["workspace"]
command = ["./target/release/herdr-tags", "ls"]

[[actions]]
id = "clear"
title = "Clear all tag tokens and release the view"
contexts = ["workspace"]
command = ["./target/release/herdr-tags", "clear"]

[[actions]]
id = "gc"
title = "Drop tag entries for panes that are gone"
contexts = ["workspace"]
command = ["./target/release/herdr-tags", "gc"]

[[actions]]
id = "filter-clear"
title = "Clear tag filters"
contexts = ["workspace"]
command = ["./target/release/herdr-tags", "filter-clear"]
```

- [ ] **Step 9: Commit**

```bash
cargo clippy -- -D warnings && cargo test
git add src/cmd.rs src/main.rs herdr-plugin.toml
git commit -m "feat: add, remove, list, delete, filter, clear, and gc commands"
```

---

### Task 6: Requirement 5 end to end

The API half is verifiable here; the visual half is not (see "Operator-only" above). Do both, and do not conflate them.

- [ ] **Step 1: Set up a two-tag, two-agent fixture**

```bash
cd ~/code/perso/herdr-tags
export HERDR_PLUGIN_STATE_DIR=~/.local/state/herdr/plugins/tags
A=$(herdr agent list | python3 -c 'import json,sys; print(json.load(sys.stdin)["result"]["agents"][0]["pane_id"])')
B=$(herdr agent list | python3 -c 'import json,sys; print(json.load(sys.stdin)["result"]["agents"][1]["pane_id"])')
./target/release/herdr-tags add keep "$A"
./target/release/herdr-tags add hide "$B"
./target/release/herdr-tags ls
```

Expected: `keep 1`, `hide 1`.

- [ ] **Step 2: Filter one tag IN, and check the emitted projection**

```bash
./target/release/herdr-tags filter keep in
```

There is **no `agent.view.get`** — the Agent method list is `list, get, read, explain, send_keys, prompt, wait, rename, focus, start, view.set, view.clear`, so the active projection cannot be read back. The `set` response printed by the command is the only machine-checkable confirmation that it was installed. What *can* be checked is that the projection did not leak into the agent inventory:

```bash
herdr agent list | python3 -c 'import json,sys; print("agent.list still returns", len(json.load(sys.stdin)["result"]["agents"]), "agents")'
```

Expected: the filter command succeeds, and `agent.list` still returns **every** agent, filtered or not. That is fact 1 working as documented — the projection is presentational, so tag counts and this plugin's own views never lie even while the sidebar hides rows.

- [ ] **Step 3: Operator check — look at the sidebar**

⚠️ This is the only way to confirm requirement 5, and it needs your eyes: no API returns the projected list.

Expected: the Agents section of the sidebar now shows **only** the agent tagged `keep`. The workspaces (Spaces) section is unaffected — the projection covers the Agents view only.

If nothing changed, check in this order: `herdr plugin list` shows `tags` enabled (herdr silently refuses `plugin:`-sourced views from unknown or disabled plugins), then the token dump from Task 5 Step 3 shows `tag_keep` on that pane.

- [ ] **Step 4: Filter a tag OUT and re-check**

```bash
./target/release/herdr-tags filter keep off
./target/release/herdr-tags filter hide out
```

Expected (operator): every agent **except** the one tagged `hide` is listed. This exercises the `not exists` branch proven at the API level in fact 4.

- [ ] **Step 5: Verify OR semantics across two included tags**

```bash
./target/release/herdr-tags filter hide off
./target/release/herdr-tags add keep "$B"
./target/release/herdr-tags filter keep in
./target/release/herdr-tags filter hide in
```

Expected (operator): both agents visible — `keep` OR `hide`, not AND. If only one shows, `build_filter` emitted `all` where it should emit `any`; Task 3's tests would have caught it, so re-run `cargo test --test view`.

- [ ] **Step 6: Verify removing a filtered tag's last agent does not blank the view**

This is the trap the last-occurrence rule in `cmd::remove` exists for. `hide` is currently filtered in and carried by exactly one agent (`$B`); take it away:

```bash
./target/release/herdr-tags filter keep off
./target/release/herdr-tags rm hide "$B"
./target/release/herdr-tags ls
```

Expected: `rm` prints a second line saying `hide` was its last agent and was dropped from the filter too, and `ls` no longer lists `hide`. Then confirm the view was released rather than left filtering on a token nothing writes:

```bash
./target/release/herdr-tags sync
herdr plugin log list --plugin tags --limit 1
```

Expected: `view_active=false`, and (operator) every agent visible. **A `view_active=true` here is the bug**: the projection would be filtering on `tag_hide`, which no pane carries any more, so the Agents view would be empty with nothing on screen explaining why.

- [ ] **Step 7: Verify the filter survives a resync**

Step 6 left `filter.json` empty, so re-establish a filter first — `keep` still exists on both agents:

```bash
./target/release/herdr-tags filter keep in
./target/release/herdr-tags sync
herdr plugin log list --plugin tags --limit 1
```

Expected: `view_active=true`. This is what the `[[startup]]` hook relies on after a server restart, since the projection is transient (fact 6).

- [ ] **Step 8: Verify clearing releases the view**

```bash
./target/release/herdr-tags filter-clear
herdr plugin action invoke tags.sync && sleep 1
herdr plugin log list --plugin tags --limit 1
```

Expected: `view_active=false`, and (operator) every agent visible again.

- [ ] **Step 9: Verify a deleted tag cannot strand the view**

```bash
./target/release/herdr-tags filter keep in
./target/release/herdr-tags delete keep
./target/release/herdr-tags ls
herdr plugin action invoke tags.sync && sleep 1 && herdr plugin log list --plugin tags --limit 1
```

Expected: `view_active=false` — deleting a filtered tag drops it from `filter.json` too, so the view is released instead of referencing a token nothing writes. A `view_active=true` here means `delete` failed to call `filter.forget`.

- [ ] **Step 10: Gate the no-print contract**

The contract from Task 4 Step 5 is prose, and prose does not fail a build. This makes it checkable before Task 7 depends on it:

```bash
! grep -rn 'println!\|eprintln!\|print!' src/cmd.rs src/reconcile.rs src/view.rs src/model.rs src/herdr.rs
```

Expected: exit 0 with no output — the `!` inverts `grep`'s "found something" exit. `src/main.rs` is deliberately excluded: it is the one place allowed to print. If this fires, a message is escaping to a stream instead of being returned, and it will corrupt the TUI's frame the moment Task 7 calls that function. `src/ui/` cannot be listed yet because it does not exist until Task 7, which re-runs this same command with `src/ui/` appended.

- [ ] **Step 11: Commit the verification**

```bash
git commit --allow-empty -m "test: verify tag filtering drives the agents view end to end"
```

---

### Task 7: The ratatui TUI

**Files:**
- Create: `src/ui/mod.rs`, `src/ui/agents.rs`, `src/ui/tags.rs`
- Modify: `src/main.rs` (wire `ui`), `src/lib.rs` (add `pub mod ui;`), `herdr-plugin.toml` (two panes)

**Interfaces:**
- Consumes: everything above.
- Produces: `ui::run(dock: bool) -> Result<(), String>`.

- [ ] **Step 1: Define the app state and reload path in `src/ui/mod.rs`**

```rust
mod agents;
mod tags;

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::DefaultTerminal;

use crate::herdr::{self, AgentInfo};
use crate::model::{FilterState, Mode, TagName, TagStore};
use crate::reconcile;
use crate::view;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Agents,
    Tags,
}

/// A modal text prompt (adding a tag) or a confirmation (deleting one).
#[derive(Debug, Clone)]
pub enum Prompt {
    AddTag { pane_id: String, buffer: String },
    RemoveTag { pane_id: String, choices: Vec<TagName>, cursor: usize },
    ConfirmDelete { tag: TagName },
}

pub struct App {
    pub dock: bool,
    pub focus: Pane,
    pub agents: Vec<AgentInfo>,
    pub labels: BTreeMap<String, String>,
    pub store: TagStore,
    pub filter: FilterState,
    pub counts: BTreeMap<TagName, usize>,
    pub known: Vec<TagName>,
    pub agent_cursor: usize,
    pub tag_cursor: usize,
    pub prompt: Option<Prompt>,
    pub status: String,
    pub quit: bool,
    /// Wall-clock of the last herd read, so input latency and refresh rate stay
    /// independent -- see the event loop.
    pub last_reload: Instant,
}

impl App {
    pub fn new(dock: bool) -> Result<Self, String> {
        let mut app = Self {
            dock,
            focus: Pane::Agents,
            agents: Vec::new(),
            labels: BTreeMap::new(),
            store: TagStore::load()?,
            filter: FilterState::load()?,
            counts: BTreeMap::new(),
            known: Vec::new(),
            agent_cursor: 0,
            tag_cursor: 0,
            prompt: None,
            status: String::new(),
            quit: false,
            last_reload: Instant::now(),
        };
        app.reload()?;
        Ok(app)
    }

    /// Re-reads the herd and the state files. Called after every mutation so a
    /// count on screen is never stale.
    pub fn reload(&mut self) -> Result<(), String> {
        self.agents = herdr::list_agents()?;
        self.labels = herdr::list_workspaces()?
            .into_iter()
            .map(|w| (w.workspace_id, w.label))
            .collect();
        self.store = TagStore::load()?;
        self.filter = FilterState::load()?;
        let live: Vec<String> = self.agents.iter().map(|a| a.pane_id.clone()).collect();
        self.counts = self.store.counts(&live);
        self.known = self.store.all_tags().into_iter().collect();
        self.agent_cursor = self.agent_cursor.min(self.agents.len().saturating_sub(1));
        self.tag_cursor = self.tag_cursor.min(self.known.len().saturating_sub(1));
        self.last_reload = Instant::now();
        Ok(())
    }

    /// Apply a mutation, then reload. **Every mutation goes through a `cmd::`
    /// function, never through `self.store` / `self.filter` directly.**
    ///
    /// Two reasons, both load-bearing. First, `self.store` is a snapshot taken
    /// at the last reload: saving it would clobber anything written since by the
    /// CLI or the other pane entrypoint — a lost update the user would see as a
    /// tag silently vanishing. The `cmd::` functions re-read, apply their delta,
    /// and save, so the read-modify-write stays tight. Second, rules like the
    /// last-occurrence filter prune live in `cmd::` already; reimplementing them
    /// here would be two copies of one invariant, drifting apart on the first
    /// change.
    ///
    /// `self.store` and `self.filter` remain read-only view state for rendering.
    fn apply<F>(&mut self, mutate: F) -> Result<(), String>
    where
        F: FnOnce() -> Result<String, String>,
    {
        // Both arms land in the footer: a rejected tag name and a successful
        // "w5:p1 += review" are equally things the user wants to read, and
        // neither may reach stdout while ratatui owns the screen.
        self.status = match mutate() {
            Ok(message) => message,
            Err(e) => e,
        };
        self.reload()
    }

    pub fn selected_agent(&self) -> Option<&AgentInfo> {
        self.agents.get(self.agent_cursor)
    }

    pub fn selected_tag(&self) -> Option<&TagName> {
        self.known.get(self.tag_cursor)
    }

    pub fn workspace_label(&self, workspace_id: &str) -> &str {
        self.labels.get(workspace_id).map(String::as_str).unwrap_or(workspace_id)
    }

    pub fn filter_summary(&self) -> String {
        view::describe(&self.filter)
    }
}
```

- [ ] **Step 2: Add the event loop and key handling to `src/ui/mod.rs`**

```rust
/// Input responsiveness and data freshness are separate concerns, so they get
/// separate intervals. Polling at `INPUT_POLL` keeps keys feeling instant;
/// re-reading the herd only every `REFRESH_EVERY` keeps a docked pane left open
/// all day from hammering the socket -- the previous single 700ms timeout did
/// both, so an idle dock issued a full `agent.list` + `workspace.list` round
/// trip every 700ms forever.
const INPUT_POLL: Duration = Duration::from_millis(200);
const REFRESH_EVERY: Duration = Duration::from_secs(2);

pub fn run(dock: bool) -> Result<(), String> {
    // `ratatui::run` is generic over the closure's return type, so it takes our
    // `Result<(), String>` directly. It also installs a panic hook that restores
    // the terminal -- hand-rolled enable_raw_mode/EnterAlternateScreen would
    // leave the user's terminal wrecked on a panic, since the teardown calls
    // never run when the stack unwinds past them.
    ratatui::run(|mut terminal| {
        let mut app = App::new(dock)?;
        event_loop(&mut terminal, &mut app)
    })
}

fn event_loop(terminal: &mut DefaultTerminal, app: &mut App) -> Result<(), String> {
    while !app.quit {
        terminal.draw(|frame| draw(frame, app)).map_err(|e| e.to_string())?;

        if event::poll(INPUT_POLL).map_err(|e| e.to_string())? {
            if let Event::Key(key) = event::read().map_err(|e| e.to_string())? {
                if key.kind == KeyEventKind::Press {
                    if app.prompt.is_some() {
                        handle_prompt_key(app, key.code)?;
                    } else {
                        handle_key(app, key.code)?;
                    }
                }
            }
            continue;
        }

        // Idle. Pick up tags added from the CLI or the other pane, but only at
        // the refresh interval -- and never mid-prompt, which would redraw a
        // half-typed tag name out from under the user.
        //
        // Non-fatal on purpose: a transient socket blip should surface in the
        // footer, not tear down a dock the user left open all day. Mutations
        // still fail loudly, through `commit`.
        if app.prompt.is_none() && app.last_reload.elapsed() >= REFRESH_EVERY {
            if let Err(e) = app.reload() {
                app.status = format!("refresh failed: {e}");
                app.last_reload = Instant::now();
            }
        }
    }
    Ok(())
}

fn draw(frame: &mut ratatui::Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(2)])
        .split(frame.area());

    match app.focus {
        Pane::Agents => agents::render(frame, chunks[0], app),
        Pane::Tags => tags::render(frame, chunks[0], app),
    }
    footer::render(frame, chunks[1], app);
}
```

Implement `footer` as a small private module in the same file rendering two lines: `app.status` when non-empty, otherwise the active filter (`app.filter_summary()`) plus a reminder that filtering changes the sidebar only; and the key hints for the focused view. Keep the key list short, and show only the keys the focused view actually accepts, since `i`/`o`/`D` are gated to the Tags view: Agents shows `Tab switch · a add · r remove · Enter focus · c clear · q quit`, Tags shows `Tab switch · i in · o out · D delete · c clear · q quit`.

**`app.dock` changes exactly one thing: that hint's quit wording** — `q close pane` when docked, `q/Esc close` in the popup, because Escape dismisses a popup but a split pane just keeps running. Nothing else branches on it. It is carried on `App` rather than passed to `footer` so the two entrypoints stay one code path; if you find yourself adding a second `if app.dock` you are building two UIs and should stop and reconsider.

- [ ] **Step 3: Implement key handling**

```rust
fn handle_key(app: &mut App, code: KeyCode) -> Result<(), String> {
    match code {
        KeyCode::Char('q') | KeyCode::Esc => app.quit = true,
        KeyCode::Tab | KeyCode::Char('\t') => {
            app.focus = if app.focus == Pane::Agents { Pane::Tags } else { Pane::Agents };
        }
        KeyCode::Char('1') => app.focus = Pane::Agents,
        KeyCode::Char('2') => app.focus = Pane::Tags,
        KeyCode::Down | KeyCode::Char('j') => move_cursor(app, 1),
        KeyCode::Up | KeyCode::Char('k') => move_cursor(app, -1),
        // Each of these clones the pane id out of `selected_agent()` on its own
        // line, deliberately: holding that `&AgentInfo` across the assignment to
        // `app.prompt` is a borrow conflict, not a style question.
        KeyCode::Char('a') => {
            let Some(pane_id) = app.selected_agent().map(|a| a.pane_id.clone()) else {
                return Ok(());
            };
            app.prompt = Some(Prompt::AddTag { pane_id, buffer: String::new() });
        }
        KeyCode::Char('r') => {
            let Some(pane_id) = app.selected_agent().map(|a| a.pane_id.clone()) else {
                return Ok(());
            };
            let choices: Vec<TagName> = app.store.tags_for(&pane_id).into_iter().collect();
            if choices.is_empty() {
                app.status = "that agent has no tags".to_string();
            } else {
                app.prompt = Some(Prompt::RemoveTag { pane_id, choices, cursor: 0 });
            }
        }
        // Gated on the Tags view on purpose: these act on `selected_tag()`,
        // which is the Tags cursor. Ungated, pressing `i` from the Agents view
        // would toggle a filter for whatever off-screen tag that cursor happens
        // to sit on -- and `D` would delete a tag the user cannot see.
        KeyCode::Char('i') if app.focus == Pane::Tags => toggle_mode(app, Mode::In)?,
        KeyCode::Char('o') if app.focus == Pane::Tags => toggle_mode(app, Mode::Out)?,
        // `apply` sets `status` from the cmd:: message, so it must not be
        // overwritten afterwards -- that message is the confirmation.
        KeyCode::Char('c') => app.apply(crate::cmd::filter_clear)?,
        KeyCode::Char('D') if app.focus == Pane::Tags => {
            if let Some(tag) = app.selected_tag().cloned() {
                app.prompt = Some(Prompt::ConfirmDelete { tag });
            }
        }
        KeyCode::Enter => {
            if app.focus == Pane::Agents {
                let Some(pane_id) = app.selected_agent().map(|a| a.pane_id.clone()) else {
                    return Ok(());
                };
                herdr::call("agent.focus", serde_json::json!({"target": &pane_id}))?;
                app.status = format!("focused {pane_id}");
            }
        }
        _ => {}
    }
    Ok(())
}

/// Toggling the mode a tag already has turns it off, so `i` on an included tag
/// is "stop including" rather than a no-op.
fn toggle_mode(app: &mut App, mode: Mode) -> Result<(), String> {
    let Some(tag) = app.selected_tag().cloned() else {
        return Ok(());
    };
    // `app.filter` is a read-only snapshot; the write goes through cmd::filter,
    // which re-reads before saving. See `App::apply`.
    let next = if app.filter.mode(&tag) == mode { Mode::Off } else { mode };
    app.apply(move || crate::cmd::filter(tag.as_str(), next))
}

fn move_cursor(app: &mut App, delta: isize) {
    let (cursor, len) = match app.focus {
        Pane::Agents => (&mut app.agent_cursor, app.agents.len()),
        Pane::Tags => (&mut app.tag_cursor, app.known.len()),
    };
    if len == 0 {
        return;
    }
    let next = (*cursor as isize + delta).rem_euclid(len as isize);
    *cursor = next as usize;
}
```

Prompt handling — `handle_prompt_key`, specified exactly because "reports the error" leaves two engineers room to disagree about whether the prompt closes:

All three follow the same shape: validate, then `app.prompt.take()` to own the values, then `app.apply(...)`. Taking before applying is not a style choice — `apply` borrows `app` mutably, so a closure still holding `&app.prompt` would not compile.

1. **`AddTag`** — printable characters append to `buffer`, `Backspace` pops one, `Esc` cancels (prompt cleared, nothing written). `Enter` runs `TagName::parse(&buffer)` **for validation only**. On `Err(message)`, put `message` in `app.status` and **leave the prompt open with the buffer intact** — the user typed `my tag` and needs to fix the space, not retype from scratch. A rejected name must never close the prompt or write anything. On `Ok`, take the prompt and `app.apply(move || cmd::add(&buffer, Some(&pane_id)))`.
2. **`RemoveTag`** — `j`/`k`/arrows move `cursor` within `choices`, `Esc` cancels. `Enter` takes the prompt, pulls `choices[cursor]` out of the owned vector, and calls `app.apply(move || cmd::remove(tag.as_str(), Some(&pane_id)))`. The last-occurrence filter prune is **not** reimplemented here: `cmd::remove` already owns that rule, which is the whole point of routing through it.
3. **`ConfirmDelete`** — `y` or `Enter` confirms; **any other key cancels**, including `n`. Confirming takes the prompt and calls `app.apply(move || cmd::delete(tag.as_str()))`, which owns both `remove_everywhere` and the `filter.forget`.

Note what these have in common: the prompt supplies *arguments*, never state mutations. `cmd::` re-reads the files, applies its delta, saves, and reconciles; `App::apply` then reloads the snapshot. Nothing in `ui/` writes `tags.json` or `filter.json`.

The idle refresh in the event loop is suppressed while any prompt is open, which is what keeps `choices` and `buffer` stable under the user's hands — without it a reload could renumber `choices` between opening the prompt and pressing `Enter`.

- [ ] **Step 4: Implement the two views**

`src/ui/agents.rs`: a `Table` (or bordered `List`) with columns `status glyph · workspace label · tab/pane · agent · tags`. Tags come from `app.store.tags_for(pane_id)`, joined with spaces and dimmed. Highlight the row at `app.agent_cursor`. Title: `Agents (N)`.

`src/ui/tags.rs`: rows of `mode glyph · tag name · count`, where the glyph is `+` for `Mode::In` (green), `−` for `Mode::Out` (red), `·` for `Mode::Off` (dim). Counts come from `app.counts`. Title: `Tags (N)`. When `app.known` is empty, render a single dim line: `no tags yet — press 1, pick an agent, press a`.

Render any active `Prompt` as a centred bordered overlay over whichever view is focused.

- [ ] **Step 5: Wire `ui` into `main.rs`**

Replace the Task 5 placeholder arm. `ui::run` returns `Result<(), String>` rather than a message, because ratatui owned the screen and the footer already said everything; mapping to an empty string is what suppresses the stray trailing `println!`.

```rust
        "ui" => herdr_tags::ui::run(args.iter().any(|a| a == "--dock")).map(|()| String::new()),
```

- [ ] **Step 6: Add both pane entrypoints to `herdr-plugin.toml`**

```toml
[[panes]]
id = "popup"
title = "Tags"
placement = "popup"
width = "70%"
height = "70%"
command = ["./target/release/herdr-tags", "ui"]

[[panes]]
id = "dock"
title = "Tags"
placement = "split"
command = ["./target/release/herdr-tags", "ui", "--dock"]

[[actions]]
id = "open"
title = "Open tags (popup)"
contexts = ["workspace"]
command = ["./target/release/herdr-tags", "open-popup"]
```

A plugin action cannot open a plugin pane by itself, so `open-popup` shells out to herdr. This is the only place the plan invokes the `herdr` binary rather than the socket, because `plugin.pane.open` is what actually spawns the pane. Add to `src/cmd.rs`:

```rust
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
```

And the dispatch arm in `src/main.rs`, beside the others:

```rust
        "open-popup" => cmd::open_popup(),
```

- [ ] **Step 7: Verify the popup**

```bash
cargo build --release
herdr plugin link ~/code/perso/herdr-tags
herdr plugin pane open --plugin tags --entrypoint popup
```

Expected (operator): a bordered TUI listing your live agents with their tags, footer showing `no filter`. Exercise it: `a` → type `review` → `Enter` adds the tag and the row updates; `2` → the Tags view shows `review 1`; `i` → the sidebar filters to that agent; `c` → filter cleared; `q` → closes.

If the popup opens and immediately exits, run `./target/release/herdr-tags ui` directly in a shell to see the error — a popup's exit takes its output with it.

- [ ] **Step 8: Verify the dock**

Run: `herdr plugin pane open --plugin tags --entrypoint dock`
Expected (operator): the same TUI as a normal split pane beside your work, and it stays open while you switch panes. Toggling a filter there re-filters the sidebar live — the case the dock placement exists for.

- [ ] **Step 9: Re-run the no-print gate, now including `src/ui/`**

```bash
! grep -rn 'println!\|eprintln!\|print!' src/cmd.rs src/reconcile.rs src/view.rs src/model.rs src/herdr.rs src/ui/
```

Expected: exit 0 with no output. This is the run that matters — Task 6's could not cover `src/ui/`, and `src/ui/` is the code that shares a screen with ratatui. A hit here is the exact defect the contract exists to prevent: raw text punched through the rendered frame.

- [ ] **Step 10: Commit**

```bash
cargo clippy -- -D warnings && cargo test
git add src/ui herdr-plugin.toml src/main.rs src/lib.rs src/cmd.rs
git commit -m "feat: ratatui tag manager with agents and tags views"
```

---

### Task 8: Sidebar row and keybinding

**Files:**
- Modify: `~/.config/herdr/config.toml`

- [ ] **Step 1: Add `$tags` to the agent rows**

The existing `[ui.sidebar.agents]` block currently carries `$folder` from the folders plugin (removed in Task 9). Replace that token with `$tags`, keeping the house comment style — a comment saying *why*, closed with the date:

```toml
# `$tags` is pane metadata written by ~/code/perso/herdr-tags: the tags on this
# agent, space-separated, truncated with a `+N` marker rather than silently cut.
# The filterable truth is one `tag_<name>` token per tag -- herdr's filter ops
# are exists/eq/in with no substring match, so a joined value cannot be filtered
# per tag, while a token per tag cannot be rendered by a static row. Both are
# written by the same reconcile, so they cannot drift.
#
# Filtering happens through `agent.view.set`, herdr's own projection over the
# Agents view: it hides rows here without touching `agent.list`, notifications,
# or attention counts. The projection is transient, so the plugin's [[startup]]
# hook re-applies it after a restart, and `tags.clear` releases it before the
# plugin is disabled. 2026-08-20.
[ui.sidebar.agents]
row_gap = 0
rows = [
  ["state_icon", { token = "$tags", dim = true }, "workspace"],
  [{ token = "terminal_title_stripped", dim = false }],
  [{ token = "$quota", dim = true }],
]
```

- [ ] **Step 2: Bind a key to the popup**

Replace the `prefix+f` folders binding (removed in Task 9) with `prefix+a` — mnemonic for agents, and free in both keymaps:

```toml
# Tag manager — popup over the layout, no columns spent. `herdr plugin pane open`
# under the hood; see ~/code/perso/herdr-tags. 2026-08-20.
[[keys.command]]
key = "prefix+a"
type = "plugin_action"
command = "tags.open"
description = "tags"
```

`prefix+a` was chosen by elimination against the full default keymap (`herdr --default-config`), which already claims `?  s  q  o  w  g  c  p  n  e  h  j  k  l  v  x  z  r  b  tab  minus  1..9` plus `shift+{r,n,g,w,d,t,x,p,tab}`, and this machine's own config claims `prefix+t` (scratch shell) and `prefix+u` (quota panel). Two chords worth naming explicitly, because both look free and are not:

1. **`prefix+g` is `goto`** — the navigator. Do not take it.
2. **`prefix+shift+t` is `rename_tab`** — it looks like the obvious "tags" chord and it is already bound.

The freed `prefix+f` is the other genuinely free option if you would rather keep `a` available.

- [ ] **Step 3: Reload and verify**

```bash
herdr server reload-config
```

Expected: `"status":"applied"` with an empty `diagnostics` array. Then (operator) agent rows show their tags, and the bound key opens the TUI.

- [ ] **Step 4: Verify the whole contract survives a restart**

Non-destructive gate first — reconciling from cleared state is exactly what the startup hook does:

```bash
herdr plugin action invoke tags.clear && sleep 1
herdr plugin action invoke tags.sync && sleep 1
herdr plugin log list --plugin tags --limit 2
```

Expected: the clear reports the tokens it removed, then the sync re-writes them and reports `view_active` matching your saved filter.

⚠️ **Operator step — do not run this from an agent inside this herd**, `herdr server stop` kills the session hosting it:

```bash
herdr server stop && herdr
herdr plugin log list --plugin tags --limit 1
```

Expected: a `[[startup]]` entry showing tokens re-written and the view re-applied, with no manual invoke.

- [ ] **Step 5: Commit the config**

```bash
cd ~/.config/herdr && yadm add config.toml && yadm commit -m "herdr: tags token on agent rows, tag manager keybinding"
```

`yadm`, not `git` — `~/.config/herdr/config.toml` is a yadm-tracked dotfile.

---

### Task 9: Documentation, and retiring herdr-folders

Do this last: tags must be proven before folders goes away.

**Files:**
- Create: `README.md`
- Modify: `~/.omp/agent/AGENTS.md`, `~/.config/herdr/config.toml`

- [ ] **Step 1: Write `README.md`**

Cover, in order:

1. What it does — tag agents, count tags, delete a tag everywhere, filter the Agents sidebar by tag — and that filtering is herdr's own `agent.view.set` projection, so it affects the sidebar, mobile Agents list, mouse targets, indexed focus and next/previous navigation, but never `agent.list`, notifications, detection, or attention counts.
2. Install: `cargo build --release` then `herdr plugin link`, and that `plugin link` runs no `[[build]]`, so the tree must be built by hand.
3. That `[[startup]]` does not run on link or enable — the first reconcile after linking is `herdr plugin action invoke tags.sync`.
4. Tag name rules: lowercased, `[a-z0-9_-]`, ≤28 characters, and why (the token key is `tag_<name>` and herdr caps keys at 32).
5. The dual token representation and why both exist.
6. Identity: tags key on **pane id**, so restarting an agent in a pane keeps its tags, and a pane id that gets reused inherits them. `gc` prunes entries whose pane is gone; nothing prunes automatically, because closing a pane must not discard tags.
7. Filter semantics: multiple included tags are OR-ed; excluded tags are AND-NOT and beat includes.
8. The `config.toml` row needed to render `$tags`, and that the row's token name is coupled to `DISPLAY_TOKEN` in `src/model.rs`.
9. The no-TTL / transient-view contract, and **`tags.clear` before disabling or unlinking** — otherwise tokens stay frozen on the rows and the projection lingers until the server exits.
10. Every command, one line each.
11. Hooked events (`pane.created`, `pane.agent_detected`) and that `pane.focused` is deliberately unhooked.
12. `herdr plugin log list --plugin tags` as where failures surface, including that a partly-rejected reconcile still applies every write it could and then exits non-zero.

- [ ] **Step 2: Retire the folders plugin**

```bash
herdr plugin action invoke folders.clear && sleep 1
herdr plugin log list --plugin folders --limit 1
```

Expected: the folders tokens are cleared. Then, since `reorder` grouped the workspaces, put the order back before unlinking — after unlinking, its `restore-order` action is gone:

```bash
python3 - <<'PY'
import json, pathlib
p = pathlib.Path.home() / ".config/herdr/plugins/config/folders/folders.json"
cfg = json.loads(p.read_text())
cfg["reorder"] = False
p.write_text(json.dumps(cfg, indent=2) + "\n")
PY
herdr plugin action invoke folders.restore-order && sleep 1
herdr plugin log list --plugin folders --limit 1
herdr plugin unlink folders
herdr plugin list
```

Expected: the restore reports the recorded order put back and the baseline consumed, then `herdr plugin list` shows only `tags`. The `~/code/perso/herdr-folders` repo and its GitHub remote stay untouched — this is a retirement, not a deletion.

- [ ] **Step 3: Drop the folders remnants from `config.toml`**

Remove the `[ui.sidebar.spaces]` block added for `$folder` (or keep the section and drop only the `$folder` token from its rows, if you like the `branch`/`git_status` row it also added), and delete the `prefix+f` → `folders.assign` binding. Then:

```bash
herdr server reload-config
```

Expected: `"status":"applied"`, empty diagnostics, and no blank gap where `$folder` used to render.

- [ ] **Step 4: Update `~/.omp/agent/AGENTS.md`**

Replace the `herdr-folders` item in the `## herdr` section with `herdr-tags`: linked from `~/code/perso/herdr-tags`, owns the `tag_*` and `tags` pane metadata tokens (alongside `quota-pace`'s `quota*`), owns the single `agent.view.set` projection under source `plugin:tags` — so any other tool setting an agent view will silently replace this plugin's filter — `plugin link` runs no build so the tree needs `cargo build --release` by hand, and `tags.clear` must run before disabling it or the tokens and projection linger.

- [ ] **Step 5: Verify docs match reality**

```bash
herdr plugin list
herdr plugin action list --plugin tags
```

Expected: only `tags`, enabled, `local:`; actions `tags.sync`, `tags.paths`, `tags.ls`, `tags.clear`, `tags.gc`, `tags.filter-clear`, `tags.open`. Re-read the README against that list and fix any drift.

- [ ] **Step 6: Commit**

```bash
cd ~/code/perso/herdr-tags && git add README.md && git commit -m "docs: herdr-tags usage, limits, and teardown"
cd ~/.config/herdr && yadm add config.toml && yadm commit -m "herdr: retire folders rows and binding"
cd ~ && yadm add .omp/agent/AGENTS.md && yadm commit -m "docs: herdr-tags replaces herdr-folders"
```

---

## Self-Review

**Spec coverage.** Requirement 1 → Task 5 Steps 1, 3. Requirement 2 → Task 5 Steps 1, 5. Requirement 3 → Task 5 Steps 1, 4 (counts fold over live agents only). Requirement 4 → Task 5 Steps 1, 6, plus Task 6 Step 8 proving a deleted tag cannot strand the view. Requirement 5 → Task 3 (composition, unit-tested), Task 6 (end to end, API half automated and visual half flagged operator-only), Task 8 Step 4 (survives restart). The TUI covering all five interactively → Task 7.

**Placeholder scan.** No TBDs. Every code step carries complete, compilable code except Task 7 Steps 3–4, where the prompt-handling arms and the two render functions are specified as behaviour plus exact widget choices rather than transcribed line by line — that is ratatui layout work whose shape is fixed by the `App` fields and `Prompt` enum already given, and writing 300 lines of widget code into a plan would be transcription, not design. Every verification step names its command and expected output.

**Type consistency.** `TagName` is the only tag representation crossing a boundary; `token_key()`/`from_token_key()` are the sole `tag_` prefix knowledge. `Mode` is shared by `FilterState`, the CLI, and the TUI. `TokenWrite { pane_id, key, value: Option<String> }` is what `plan_tokens` returns and `apply` consumes. `build_filter` returns `Option<Value>` and both callers (`apply`, and the TUI through `apply`) treat `None` as "clear the view", never "set an empty filter". `AgentInfo` field names match fact 9 exactly, with `#[serde(default)]` on every non-required one.

**Residual risks, recorded not solved.**
1. **The agent view is a singleton.** Any other tool calling `agent.view.set` atomically replaces this plugin's filter with no notification. Nothing on this machine does today; the next `sync` restores it. Documented in the README and AGENTS.md.
2. **Pane-id identity drifts.** The user chose it knowing the tradeoff; `seen_as` records provenance for a human, and `gc` cleans up deliberately. A pane id reused by an unrelated agent inherits tags.
3. **Requirement 5's visual confirmation is operator-only.** No API returns the projected list; this is stated up front rather than papered over with an `active:true` response.
4. **Token budget.** 32 tokens per resource, minus quota-pace's 5 and one display token, leaves ~25 tags on a single agent. Exceeding it makes herdr reject the write with `metadata_token_limit`, which `apply` reports as a failure rather than crashing.
5. **A foreign source writing a `tag_*` key would cause a repeated no-op.** Pane tokens from all sources merge into one map, so `plan_tokens` would see a `tag_x` it does not own, decide it is stale, and clear it under `plugin:tags` — which does not remove another source's token. The result is one wasted write per reconcile, not corruption or a loop, since nothing re-triggers on it (`pane.updated` is not hookable). Nothing on this machine writes that prefix; recorded rather than defended against.
