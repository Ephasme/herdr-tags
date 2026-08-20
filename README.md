# herdr-tags

Tag herdr agents, and filter the Agents sidebar by tag.

## What it does

1. Tag an agent, remove a tag from one agent, list every tag with a live count,
   delete a tag from every agent at once.
2. Filter the Agents view by tag — include some, exclude others.

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

`$tags` needs a row in `~/.config/herdr/config.toml`:

```toml
[ui.sidebar.agents]
rows = [
  ["state_icon", { token = "$tags", dim = true }, "workspace"],
]
```

That token name is coupled to `DISPLAY_TOKEN` in `src/model.rs`. Change one
without the other and the row silently renders blank.

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
| `ui [--dock]` | the TUI; `--dock` only changes the quit wording |
| `open-popup` | re-enter herdr to open the popup pane (bindable) |

Every command prints its result to stdout and returns non-zero only on failure.
Nothing below `src/cmd.rs` prints: the TUI calls the same functions inside
ratatui's alternate screen, where a stray `println!` would corrupt the frame.

## Keys in the TUI

`Tab`/`1`/`2` switch views · `j`/`k` move · `a` add · `r` remove · `Enter` focus
that agent · `i` include · `o` exclude · `D` delete everywhere · `c` clear
filters · `q` quit.

`i`, `o` and `D` are gated to the Tags view on purpose: they act on the Tags
cursor, so from the Agents view they would target a tag you cannot see.

## Hooked events

`pane.created` and `pane.agent_detected` — a fresh pane carries no tokens, so
re-stamp when one appears or an agent is detected in it.

`pane.focused` is deliberately **not** hooked: it fires constantly.
`pane.updated` is not hookable at all, which is why writing tokens cannot feed
this plugin its own events.

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
