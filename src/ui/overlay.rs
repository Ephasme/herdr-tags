use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use super::{App, Prompt};

/// Renders the active prompt as a centred bordered box over whichever view is
/// focused. Both views call this last, so the overlay always wins the z-order.
pub fn render(frame: &mut ratatui::Frame, area: Rect, app: &App) {
    let Some(prompt) = app.prompt.as_ref() else {
        return;
    };

    let (title, body) = match prompt {
        Prompt::AddTag { pane_id, buffer } => (
            format!("Add tag to {pane_id}"),
            vec![
                // A trailing bar shows where the next character lands; the
                // terminal cursor is parked elsewhere by the frame.
                Line::from(format!("{buffer}|")),
                Line::styled(
                    "Enter add · Backspace · Esc cancel",
                    Style::default().add_modifier(Modifier::DIM),
                ),
            ],
        ),
        Prompt::RemoveTag { pane_id, choices, cursor } => {
            let mut lines: Vec<Line> = choices
                .iter()
                .enumerate()
                .map(|(index, tag)| {
                    let marker = if index == *cursor { ">" } else { " " };
                    Line::from(Span::raw(format!("{marker} {}", tag.as_str())))
                })
                .collect();
            lines.push(Line::styled(
                "j/k move · Enter remove · Esc cancel",
                Style::default().add_modifier(Modifier::DIM),
            ));
            (format!("Remove tag from {pane_id}"), lines)
        }
        Prompt::ConfirmDelete { tag } => (
            "Delete tag everywhere".to_string(),
            vec![
                Line::from(format!("delete `{}` from every agent?", tag.as_str())),
                Line::styled(
                    "y or Enter confirm · any other key cancels",
                    Style::default().add_modifier(Modifier::DIM),
                ),
            ],
        ),
    };

    let height = (body.len() as u16).saturating_add(2).min(area.height);
    let box_area = centre(area, 60, height);

    // `Clear` first, or the view underneath bleeds through the box.
    frame.render_widget(Clear, box_area);
    frame.render_widget(
        Paragraph::new(body).block(Block::default().borders(Borders::ALL).title(title)),
        box_area,
    );
}

fn centre(area: Rect, width_percent: u16, height: u16) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(height),
            Constraint::Min(0),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - width_percent) / 2),
            Constraint::Percentage(width_percent),
            Constraint::Percentage((100 - width_percent) / 2),
        ])
        .split(vertical[1])[1]
}
