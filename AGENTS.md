# herdr-tags — agent notes

User-facing behavior lives in `README.md`; read it first. This file is
operational guidance for an agent editing this repo — the things that will
silently break, or silently not fire, and are not obvious from any one file.

## What this is

A herdr plugin: a Rust binary (`src/main.rs`) invoked by the herdr server as
CLI commands, `[[events]]` hooks, `[[actions]]`, and — via `ui`/`ui --edit` —
a ratatui TUI rendered inside a herdr pane. `herdr-plugin.toml` is the
manifest; it declares every entrypoint the server can invoke.

## Source layout

```
src/
  main.rs       command dispatch, the only place stdout/stderr is touched
  cmd.rs        every mutation and every CLI command; App::apply calls these
  model.rs      TagName, TagStore, FilterState — parsing, storage, invariants
  reconcile.rs  desired state -> herdr token writes + agent.view.set diff
  view.rs       FilterState -> agent.view.set filter JSON
  herdr.rs      the only file that touches the herdr socket API
  complete.rs   pure: autocomplete suggestion matcher
  layout.rs     pure: chip-row and suggestion-slot sizing arithmetic
  ui/
    mod.rs      App, Prompt, the event loop, all key handling
    agents.rs   Agents view (table)
    tags.rs     Tags view (list, filter state)
    overlay.rs  the single render site for both Prompt variants
```

## Hard invariants

1. **Nothing the TUI calls may print.** `src/ui/` shares the screen with
   ratatui; a stray `println!`/`eprintln!`/`print!` in `cmd.rs`,
   `reconcile.rs`, `view.rs`, `model.rs`, `herdr.rs`, `complete.rs`,
   `layout.rs`, or anything under `src/ui/` corrupts the frame. `main.rs` is
   the *only* place output happens — verify with:

   ```bash
   ! grep -rn 'println!\|eprintln!\|print!' \
     src/cmd.rs src/reconcile.rs src/view.rs src/model.rs src/herdr.rs \
     src/complete.rs src/layout.rs src/ui/
   ```

2. **Every test is an integration test in `tests/`, against a `pub` module.**
   There is no in-file `#[cfg(test)]` anywhere in this crate — that's not an
   oversight, it's the convention. New pure logic (no ratatui types, no I/O)
   gets its own `pub mod` registered in `src/lib.rs` and its own
   `tests/<module>.rs`, exactly like `complete.rs`/`tests/complete.rs` and
   `layout.rs`/`tests/layout.rs`. Write the test file first — confirm it
   fails to compile — then the implementation.

3. **Mutations never touch `App.store` / `App.filter` directly.** Both are
   read-only snapshots taken at the last reload. Every mutation routes
   through a `cmd::` function via `App::apply` (`src/ui/mod.rs`), which
   re-reads, applies its delta, saves, and reconciles — see that function's
   own doc comment for why a direct write risks clobbering a concurrent CLI
   write.

4. **`DISPLAY_TOKEN` (`src/model.rs`) and the `$tags` token name used in
   `~/.config/herdr/config.toml` are coupled.** Change one without the other
   and the sidebar row silently renders blank — nothing errors.

5. **Do not run `cargo fmt`.** This tree is hand-formatted to a denser style
   than rustfmt's default (see `cmd.rs` for examples: unwrapped `.map().collect()`
   chains, a different import-sort order). `cargo fmt --check` reports diffs
   across files nobody touched this session; that's the established state,
   not drift to fix.

## Build / verify

```bash
cargo test                              # 37 tests across 5 files as of 2026-08-20
cargo clippy --all-targets -- -D warnings
# the no-print gate above
cargo build --release
herdr plugin link ~/code/perso/herdr-tags   # after any herdr-plugin.toml change
```

`herdr plugin link` runs no `[[build]]` step and re-links are a plain
overwrite (`plugins.insert`, no already-linked refusal) — always safe to
re-run after editing the manifest.

## Live-testing the TUI

Needs two env vars herdr normally injects automatically:
`HERDR_SOCKET_PATH=~/.config/herdr/herdr.sock` and
`HERDR_PLUGIN_STATE_DIR=~/.local/state/herdr/plugins/tags`. Launch
`./target/release/herdr-tags ui` under a PTY (the `hub` tool, `pty: true`,
ready-log `"Agents"`).

**The PTY log read does not reconstruct a screen.** ratatui redraws by
diffing: only changed cells are rewritten, via cursor-positioning escapes.
`hub`'s `logs` op strips those escapes and concatenates the remaining
printable bytes in write order — it does not replay cursor movement. The
*first* full frame happens to read correctly because nothing existed to diff
against; every frame after that appears as a disconnected trailing fragment
wherever its changed cells happened to land in the byte stream, not where
they land on screen. A brand-new, non-overlapping string (a fresh footer
message on an unrelated topic) still reads intact; a one-character diff
against previous content (`+` → `-` in an otherwise identical string) usually
will not show up as a contiguous match.

**Verify against ground truth instead**, especially for anything after the
first frame:

```bash
cat "$HERDR_PLUGIN_STATE_DIR/tags.json"
herdr agent list | python3 -c 'import json,sys
for a in json.load(sys.stdin)["result"]["agents"]:
    t={k:v for k,v in (a.get("tokens") or {}).items() if k.startswith("tag")}
    if t: print(a["pane_id"], t)'
```

**`hub`'s `keys` list has no Backspace/Delete.** Send the raw `0x7f` byte
through the `eval` tool instead — `tool.hub(...)` from Python/JS can carry a
literal control byte in a string; the `hub` tool call's own `text` parameter
cannot:

```python
tool.hub({"op": "send", "name": "<pty-name>", "text": "\x7f", "enter": False})
```

**Restore any tags you add.** There's no sandboxed herd — a live PTY run
touches the real `tags.json` and real herd tokens on whatever agents you pick.
Note their tags before you start, and put them back after.

## Which surface a request means

"the agent list" / "tags" / "title", said by the user, almost always means
**`~/.config/herdr/config.toml`'s `[ui.sidebar.agents].rows`** — the sidebar
they look at constantly — not this plugin's own popup/dock TUI table
(`src/ui/agents.rs`), which is a secondary surface rarely opened. When a
request could mean either, check `config.toml` first. Tense is a strong
signal: a request describing something *not yet true* almost certainly points
at the real target, not a surface where it's already the case.

Config edits split by how they take effect:

- `[ui.*]` display config (sidebar rows, colors): `herdr server reload-config`
  is enough — confirm `"status": "applied"` in its JSON response.
- `[[keys.command]]` bindings: `reload-config` is **not** enough; the running
  server needs a live handoff (see `README.md`'s "Binding the shortcuts"
  section for the exact socket call). An invalid `type` (must be exactly
  `shell` / `pane` / `popup`) is accepted with no diagnostic by both
  `config check` and `reload-config`, and then the binding never fires —
  check the value, not just that the reload succeeded.

## Plans

`docs/plans/`, one file per work session, named `YYYY-MM-DD-<slug>.md`.
