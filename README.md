# herdr-tags

Tag herdr agents, filter the Agents sidebar by tag, and edit an agent's tags
from a small popup with autocomplete.

## What it does

1. Tag an agent, remove a tag from one agent, list every tag with a live
   count, delete a tag from every agent at once.
2. Filter the Agents view by tag — include some, exclude others.
3. Edit one agent's tags in a popup: chips you can step through and remove,
   plus a field that autocompletes over every tag you've already used.

Filtering is **herdr's own `agent.view.set` projection**, not a local view. It
controls the expanded and collapsed sidebar, the mobile Agents list, mouse
targets, indexed focus, and next/previous agent navigation. It never touches
`agent.list`, notifications, agent detection, or attention counts — so tag
counts and this plugin's own views stay honest even while the sidebar hides
rows.

## Install

```bash
cargo build --release
herdr plugin link ~/code/perso/herdr-tags
herdr plugin action invoke tags.sync
```

`herdr plugin link` runs **no** `[[build]]` step, so the tree must be built by
hand. And `[[startup]]` does not fire on link or enable, so that first
`tags.sync` is the manual reconcile that puts tokens and the projection in
place.

## Tag names

Lowercased and trimmed, `[a-z0-9_-]` only, at most **28 characters**. The
filterable token key is `tag_<name>` and herdr caps a metadata key at 32
characters, so `tag_` eats four of them. A rejected name is reported and
nothing is written — the TUI keeps the prompt open with your text intact.

## Two tokens per agent, on purpose

| Token | Shape | Why |
|---|---|---|
| `tag_<name>` | one per tag, value `1` | filterable — herdr's filter ops are `exists`/`eq`/`in`, with no substring match |
| `tags` | one joined, space-separated | renderable — a static sidebar row cannot expand a set of keys |

Neither alone is sufficient: a joined value cannot be filtered per tag, and a
token per tag cannot be rendered by a fixed row. Both are written by the same
reconcile pass, so they cannot drift apart. The joined value is truncated at 80
characters with a visible `+N` marker rather than being silently cut.

## Identity: the pane

Tags key on **pane id**. Consequences, both deliberate:

1. Restarting an agent in a pane **keeps** its tags — the tag describes the slot
   in the herd, not the process.
2. A pane id that gets reused by an unrelated agent **inherits** them.

`tags.gc` prunes entries whose pane no longer exists. Nothing prunes
automatically, because closing a pane must not discard its tags. Each entry also
records a non-authoritative `seen_as` (workspace, cwd, agent) purely so a human
reading `tags.json` can tell what a stale entry used to be.

## Filter semantics

1. Multiple **included** tags are OR-ed — `keep` or `hide`, not both.
2. **Excluded** tags are AND-NOT, and beat includes: they sit as sibling clauses
   of the same `all`.
3. Removing or deleting a filtered tag's **last** agent drops it from the filter
   too. Without that, the view would keep filtering on a token nothing writes,
   emptying the Agents list with nothing on screen to explain why.

## Rendering the row

`$tags` needs a row in `~/.config/herdr/config.toml`. It reads well sharing a
row with the terminal title, after it:

```toml
[ui.sidebar.agents]
rows = [
  ["state_icon", "workspace"],
  [{ token = "terminal_title_stripped", dim = false }, { token = "$tags", dim = true }],
]
```

`$tags` also stands alone, on its own row or anywhere else that takes a
static token. That token name is coupled to `DISPLAY_TOKEN` in `src/model.rs`
— change one without the other and the row silently renders blank.

A config edit like this needs `herdr server reload-config` to take effect —
confirm with `"status": "applied"` in its output.

## The tag editor

Two ways in:

1. Press `a` from the Agents view in the overview popup/dock.
2. A bindable action (`tags.edit`, see below) that opens a dedicated, smaller
   popup scoped to whichever agent currently has focus — no need to open the
   overview first.

The box shows:

```
name ✕ name ✕ +2
add: buffer|
suggestion
suggestion
Tab complete · Enter save · ←→ ✕ Backspace · Esc close
```

1. **Chip row** — the agent's current tags as `name ✕`. Truncates to whole
   chips with a trailing `+N`, never a cut name.
2. **Add field** — type a tag; suggestions below autocomplete over every tag
   you've used elsewhere, minus what this agent already has.
3. **Suggestions** — `Tab` accepts the highlighted one, `Up`/`Down` move the
   highlight.
4. **Hint** — always visible, even in a short pane; suggestion rows give way
   first.

Keys:

| Key | Does |
|---|---|
| any printable char | type into the add field |
| `Tab` | accept the highlighted suggestion |
| `Up`/`Down` | move the suggestion highlight |
| `Enter` | save the typed tag; a rejected name stays in the field, nothing is written |
| `Left`/`Right` (field empty) | step onto a chip |
| `Backspace` (field empty, chip selected) | remove that chip |
| `Backspace` (field empty, nothing selected) | does nothing — never deletes by accident |
| `Backspace` (field non-empty) | erase one character |
| `Esc` | close |

A stray `Backspace` on a freshly opened editor can never remove a tag: nothing
is selected until you explicitly step onto a chip with `Left`/`Right`, and
typing clears that selection again.

## Binding the shortcuts

Two actions, two panes: `tags.open` is the full overview (add/remove/filter,
everything below), `tags.edit` is the single-agent editor above, scoped to
whatever's focused. Example:

```toml
[[keys.command]]
key = "prefix+a"
type = "shell"
command = "herdr plugin action invoke tags.open"
description = "tags"

[[keys.command]]
key = "prefix+shift+a"
type = "shell"
command = "herdr plugin action invoke tags.edit"
description = "edit agent tags"
```

Two gotchas that fail silently:

1. `type` **must** be exactly `shell`, `pane`, or `popup`. Anything else is
   accepted with no error by both `herdr config check` and
   `herdr server reload-config`, and then the binding never fires.
2. `herdr server reload-config` is **not enough** for a new or changed
   binding — the running server must re-read keys another way. Use a live
   handoff instead, which preserves panes and agents:

   ```bash
   python3 - <<'PY'
   import json, socket, os
   s = socket.socket(socket.AF_UNIX); s.settimeout(20)
   s.connect(os.path.expanduser("~/.config/herdr/herdr.sock"))
   s.send((json.dumps({"id":"ho","method":"server.live_handoff","params":{}})+"\n").encode())
   print(s.recv(65536).decode().strip())
   PY
   ```

## Commands

| Command | Does |
|---|---|
| `add <tag> [pane]` | tag an agent (defaults to the focused one) |
| `rm <tag> [pane]` | remove one tag from one agent |
| `ls` | every tag with its live count and filter mode |
| `delete <tag>` | remove a tag from every agent, and from the filter |
| `filter <tag> <in\|out\|off>` | include, exclude, or stop filtering on a tag |
| `filter-clear` | drop every filter and release the projection |
| `sync` | re-apply tokens and the projection from the state files |
| `clear` | remove every token and release the projection |
| `gc` | drop entries for panes that no longer exist |
| `paths` | print the two state file paths |
| `ui [--dock] [--edit]` | the TUI; `--dock` only changes the quit wording, `--edit` opens straight into the tag editor for the focused agent |
| `open-popup` | re-enter herdr to open the overview popup pane (bindable) |
| `open-editor` | re-enter herdr to open the single-agent editor popup (bindable) |

Every command prints its result to stdout and returns non-zero only on failure.
Nothing below `src/cmd.rs` prints: the TUI calls the same functions inside
ratatui's alternate screen, where a stray `println!` would corrupt the frame.

## Keys in the overview TUI

**Agents view:** `Tab`/`1`/`2` switch views · `j`/`k` move · `a` open the tag
editor for the selected agent · `Enter` focus that agent · `c` clear filters ·
`q` quit.

**Tags view:** `Tab`/`1`/`2` switch views · `j`/`k` move · `i` include ·
`o` exclude · `D` delete everywhere · `c` clear filters · `q` quit.

`i`, `o` and `D` are gated to the Tags view on purpose: they act on the Tags
cursor, so from the Agents view they would target a tag you cannot see.
Adding and removing tags both live inside the tag editor now (`a`) — there is
no separate remove prompt.

## Hooked events

`pane.created` and `pane.agent_detected` — a fresh pane carries no tokens, so
re-stamp when one appears or an agent is detected in it.

`pane.focused` is deliberately **not** hooked: it fires constantly.
`pane.updated` is not hookable at all, which is why writing tokens here cannot
feed this plugin its own events.

## Where failures surface

```bash
herdr plugin log list --plugin tags
```

A partly-rejected reconcile still applies every write it could, then reports
what failed. `sync` exits non-zero in that case with the summary and the joined
failures on stderr; the mutating commands append the warning to their stdout
message and exit 0, because the state file — the part that survives a restart —
was written correctly and the next `sync` retries the token.

## Known limits

1. **The agent view is a singleton.** Any other tool calling `agent.view.set`
   atomically replaces this plugin's filter, with no notification. The next
   `sync` restores it.
2. **Pane-id identity drifts** if a pane id is reused, per the identity section
   above.
3. **No API returns the projected list**, so confirming that the sidebar really
   hid the right rows is an operator's eyes-on check.
4. **The tag editor popup is a singleton too** — herdr keeps one
   `state.popup_pane` slot. Opening `tags.edit` while a tags popup is already
   open either replaces it or is refused (`ui_busy`, logged); either way, `a`
   inside an already-open popup reaches the same editor with no second popup
   needed.

## ⚠️ Run `tags.clear` before disabling or unlinking

Tag tokens carry **no TTL**, and the projection is owned by this plugin. Neither
expires on its own:

1. Disable the plugin without clearing and the tokens stay frozen on the rows,
   with nothing maintaining them, until the server exits.
2. The projection lingers the same way, still hiding rows.

`tags.clear` removes both. It sweeps `pane.list` rather than `agent.list` — a
pane that carried tags and later stopped hosting a detected agent still holds
the tokens. It leaves the state files untouched, so `tags.sync` puts everything
back.
