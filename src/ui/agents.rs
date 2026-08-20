use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Row, Table};

use super::{overlay, App};

/// `status glyph · workspace label · tab/pane · agent · tags`.
pub fn render(frame: &mut ratatui::Frame, area: Rect, app: &App) {
    let dim = Style::default().add_modifier(Modifier::DIM);
    let selected = Style::default().add_modifier(Modifier::REVERSED);

    let rows: Vec<Row> = app
        .agents
        .iter()
        .enumerate()
        .map(|(index, agent)| {
            let tags = app.store.tags_for(&agent.pane_id);
            let joined = tags
                .iter()
                .map(|t| t.as_str())
                .collect::<Vec<_>>()
                .join(" ");

            let row = Row::new(vec![
                Line::from(status_glyph(agent.agent_status.as_deref())),
                Line::from(app.workspace_label(&agent.workspace_id).to_string()),
                Line::from(agent.pane_id.clone()),
                Line::from(agent.agent.clone().unwrap_or_else(|| "-".to_string())),
                Line::from(Span::styled(joined, dim)),
            ]);
            if index == app.agent_cursor {
                row.style(selected)
            } else {
                row
            }
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(2),
            Constraint::Length(16),
            Constraint::Length(10),
            Constraint::Length(8),
            Constraint::Min(10),
        ],
    )
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!("Agents ({})", app.agents.len())),
    );

    frame.render_widget(table, area);
    overlay::render(frame, area, app);
}

/// herdr's own vocabulary: blocked needs attention, working is busy, done is
/// finished, idle is waiting.
fn status_glyph(status: Option<&str>) -> &'static str {
    match status {
        Some("blocked") => "!",
        Some("working") => ">",
        Some("done") => "=",
        Some("idle") => "·",
        _ => " ",
    }
}
