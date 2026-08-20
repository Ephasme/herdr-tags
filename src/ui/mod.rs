mod agents;
mod overlay;
mod tags;

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::DefaultTerminal;

use crate::herdr::{self, AgentInfo};
use crate::model::{FilterState, Mode, TagName, TagStore};
use crate::view;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Agents,
    Tags,
}

/// A modal prompt for editing an agent's tags, or a confirmation for
/// deleting one everywhere.
#[derive(Debug, Clone)]
pub enum Prompt {
    EditAgent {
        pane_id: String,
        buffer: String,
        /// `None` until the user moves onto a chip with Left/Right. Removal
        /// requires an explicit selection, so a stray Backspace on a freshly
        /// opened editor can never delete a tag.
        tag_cursor: Option<usize>,
        suggestion_cursor: usize,
    },
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
    /// CLI or the other pane entrypoint -- a lost update the user would see as a
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

    /// Both borrows share a lifetime because the fallback returns the argument
    /// itself when a workspace has no label recorded.
    pub fn workspace_label<'a>(&'a self, workspace_id: &'a str) -> &'a str {
        self.labels.get(workspace_id).map(String::as_str).unwrap_or(workspace_id)
    }

    pub fn filter_summary(&self) -> String {
        view::describe(&self.filter)
    }
}

/// Input responsiveness and data freshness are separate concerns, so they get
/// separate intervals. Polling at `INPUT_POLL` keeps keys feeling instant;
/// re-reading the herd only every `REFRESH_EVERY` keeps a docked pane left open
/// all day from hammering the socket -- a single shared timeout would do both,
/// so an idle dock would issue a full `agent.list` + `workspace.list` round trip
/// several times a second forever.
const INPUT_POLL: Duration = Duration::from_millis(200);
const REFRESH_EVERY: Duration = Duration::from_secs(2);

pub fn run(dock: bool, edit: bool) -> Result<(), String> {
    // `ratatui::run` is generic over the closure's return type, so it takes our
    // `Result<(), String>` directly. It also installs a panic hook that restores
    // the terminal -- hand-rolled enable_raw_mode/EnterAlternateScreen would
    // leave the user's terminal wrecked on a panic, since the teardown calls
    // never run when the stack unwinds past them.
    ratatui::run(|terminal| {
        let mut app = App::new(dock)?;
        if edit {
            // A popup pane gets no `HERDR_PANE_ID` (see `cmd::focused_pane`'s
            // own doc comment), so this lands on the plugin-context or
            // focused-agent tiers -- both of which name the user's agent
            // rather than the popup itself.
            match crate::cmd::focused_pane() {
                Ok(pane_id) => match app.agents.iter().position(|a| a.pane_id == pane_id) {
                    Some(index) => {
                        app.agent_cursor = index;
                        app.prompt = Some(Prompt::EditAgent {
                            pane_id,
                            buffer: String::new(),
                            tag_cursor: None,
                            suggestion_cursor: 0,
                        });
                    }
                    // Never silently edit a different pane: leave the
                    // editor closed on the Agents list so the user can
                    // pick with `a`.
                    None => app.status = format!("{pane_id} is not an agent"),
                },
                // Never abort the TUI: a failed resolution still leaves a
                // usable Agents list.
                Err(message) => app.status = message,
            }
        }
        event_loop(terminal, &mut app)
    })
}

fn event_loop(terminal: &mut DefaultTerminal, app: &mut App) -> Result<(), String> {
    while !app.quit {
        terminal.draw(|frame| draw(frame, app)).map_err(|e| e.to_string())?;

        if event::poll(INPUT_POLL).map_err(|e| e.to_string())? {
            if let Event::Key(key) = event::read().map_err(|e| e.to_string())?
                && key.kind == KeyEventKind::Press
            {
                if app.prompt.is_some() {
                    handle_prompt_key(app, key.code)?;
                } else {
                    handle_key(app, key.code)?;
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
        // still fail loudly, through `apply`.
        if app.prompt.is_none()
            && app.last_reload.elapsed() >= REFRESH_EVERY
            && let Err(e) = app.reload()
        {
            app.status = format!("refresh failed: {e}");
            app.last_reload = Instant::now();
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

/// Two lines: what just happened (or the active filter), and the keys the
/// focused view actually accepts.
mod footer {
    use ratatui::layout::Rect;
    use ratatui::style::{Modifier, Style};
    use ratatui::text::{Line, Span};
    use ratatui::widgets::Paragraph;

    use super::{App, Pane};

    pub fn render(frame: &mut ratatui::Frame, area: Rect, app: &App) {
        let dim = Style::default().add_modifier(Modifier::DIM);

        let top = if app.status.is_empty() {
            Line::from(vec![
                Span::raw(app.filter_summary()),
                Span::styled(
                    "  (filtering changes the sidebar only)",
                    dim,
                ),
            ])
        } else {
            // A cmd:: message may be multi-line; the footer has one line, so
            // show the first and leave the rest to the CLI.
            Line::from(app.status.lines().next().unwrap_or_default().to_string())
        };

        // `app.dock` changes exactly one thing: the quit wording. Escape
        // dismisses a popup, but a split pane just keeps running.
        let quit = if app.dock { "q close pane" } else { "q/Esc close" };
        let keys = match app.focus {
            Pane::Agents => {
                format!("Tab switch · a tags · Enter focus · c clear · {quit}")
            }
            Pane::Tags => format!("Tab switch · i in · o out · D delete · c clear · {quit}"),
        };

        frame.render_widget(
            Paragraph::new(vec![top, Line::styled(keys, dim)]),
            area,
        );
    }
}

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
            app.prompt = Some(Prompt::EditAgent {
                pane_id,
                buffer: String::new(),
                tag_cursor: None,
                suggestion_cursor: 0,
            });
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
        KeyCode::Enter if app.focus == Pane::Agents => {
            let Some(pane_id) = app.selected_agent().map(|a| a.pane_id.clone()) else {
                return Ok(());
            };
            herdr::call("agent.focus", serde_json::json!({"target": &pane_id}))?;
            app.status = format!("focused {pane_id}");
        }
        _ => {}
    }
    Ok(())
}

/// Every arm follows one shape: validate, `take()` the prompt to own its
/// values, then `apply`. Taking before applying is not style -- `apply` borrows
/// `app` mutably, so a closure still holding `&app.prompt` would not compile.
///
/// The prompt supplies *arguments*, never state mutations: `cmd::` re-reads,
/// applies its delta, saves, and reconciles. Nothing here writes a state file.
fn handle_prompt_key(app: &mut App, code: KeyCode) -> Result<(), String> {
    let Some(prompt) = app.prompt.clone() else {
        return Ok(());
    };

    match prompt {
        Prompt::EditAgent { pane_id, mut buffer, tag_cursor, mut suggestion_cursor } => {
            let applied: Vec<TagName> = app.store.tags_for(&pane_id).into_iter().collect();
            match code {
                KeyCode::Esc => app.prompt = None,
                KeyCode::Char(c) => {
                    buffer.push(c);
                    app.prompt = Some(Prompt::EditAgent {
                        pane_id,
                        buffer,
                        tag_cursor: None,
                        suggestion_cursor: 0,
                    });
                }
                KeyCode::Backspace if !buffer.is_empty() => {
                    buffer.pop();
                    app.prompt = Some(Prompt::EditAgent {
                        pane_id,
                        buffer,
                        tag_cursor,
                        suggestion_cursor: 0,
                    });
                }
                KeyCode::Backspace => {
                    let Some(i) = tag_cursor else { return Ok(()) };
                    let Some(tag) = applied.get(i).cloned() else { return Ok(()) };
                    app.prompt = Some(Prompt::EditAgent {
                        pane_id: pane_id.clone(),
                        buffer,
                        tag_cursor: None,
                        suggestion_cursor,
                    });
                    app.apply(move || crate::cmd::remove(tag.as_str(), Some(&pane_id)))?;
                }
                KeyCode::Tab => {
                    let suggestions = crate::complete::suggest(&app.known, &applied, &buffer);
                    let Some(tag) = suggestions.get(suggestion_cursor) else { return Ok(()) };
                    buffer = tag.as_str().to_string();
                    app.prompt =
                        Some(Prompt::EditAgent { pane_id, buffer, tag_cursor, suggestion_cursor });
                }
                KeyCode::Down => {
                    let len = crate::complete::suggest(&app.known, &applied, &buffer).len();
                    if len == 0 {
                        return Ok(());
                    }
                    suggestion_cursor = (suggestion_cursor + 1).min(len - 1);
                    app.prompt =
                        Some(Prompt::EditAgent { pane_id, buffer, tag_cursor, suggestion_cursor });
                }
                KeyCode::Up => {
                    let len = crate::complete::suggest(&app.known, &applied, &buffer).len();
                    if len == 0 {
                        return Ok(());
                    }
                    suggestion_cursor = suggestion_cursor.saturating_sub(1);
                    app.prompt =
                        Some(Prompt::EditAgent { pane_id, buffer, tag_cursor, suggestion_cursor });
                }
                KeyCode::Right if buffer.is_empty() && !applied.is_empty() => {
                    let last = applied.len() - 1;
                    let next = match tag_cursor {
                        None => 0,
                        Some(i) => (i.min(last) + 1).min(last),
                    };
                    app.prompt = Some(Prompt::EditAgent {
                        pane_id,
                        buffer,
                        tag_cursor: Some(next),
                        suggestion_cursor,
                    });
                }
                KeyCode::Left if buffer.is_empty() && !applied.is_empty() => {
                    let last = applied.len() - 1;
                    let next = match tag_cursor {
                        None => last,
                        Some(i) => i.min(last).saturating_sub(1),
                    };
                    app.prompt = Some(Prompt::EditAgent {
                        pane_id,
                        buffer,
                        tag_cursor: Some(next),
                        suggestion_cursor,
                    });
                }
                KeyCode::Enter => {
                    // Validation only -- `cmd::add` parses again on the
                    // authoritative path. A rejected name must never close
                    // the prompt or write anything: the user typed `my tag`
                    // and needs to fix the space, not retype from scratch.
                    if let Err(message) = TagName::parse(&buffer) {
                        app.status = message;
                        return Ok(());
                    }
                    let typed = buffer;
                    // The editor stays open -- reset to an empty buffer --
                    // so a second tag can be added immediately. This
                    // assignment must happen before `apply`, which borrows
                    // `app` mutably.
                    app.prompt = Some(Prompt::EditAgent {
                        pane_id: pane_id.clone(),
                        buffer: String::new(),
                        tag_cursor: None,
                        suggestion_cursor: 0,
                    });
                    app.apply(move || crate::cmd::add(&typed, Some(&pane_id)))?;
                }
                _ => {}
            }
        }
        // `y` or Enter confirms; any other key cancels, including `n`.
        Prompt::ConfirmDelete { tag } => match code {
            KeyCode::Char('y') | KeyCode::Enter => {
                app.prompt = None;
                app.apply(move || crate::cmd::delete(tag.as_str()))?;
            }
            _ => app.prompt = None,
        },
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
