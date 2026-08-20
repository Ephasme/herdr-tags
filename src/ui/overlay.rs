use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::complete;
use crate::layout;
use crate::model::TagName;

use super::{App, Prompt};

/// Renders the active prompt as a centred bordered box over whichever view is
/// focused. Both views call this last, so the overlay always wins the z-order.
pub fn render(frame: &mut ratatui::Frame, area: Rect, app: &App) {
    let Some(prompt) = app.prompt.as_ref() else {
        return;
    };

    let dim = Style::default().add_modifier(Modifier::DIM);
    let reversed = Style::default().add_modifier(Modifier::REVERSED);

    let (title, body, width_percent) = match prompt {
        Prompt::EditAgent { pane_id, buffer, tag_cursor, suggestion_cursor } => {
            let label = app
                .agents
                .iter()
                .find(|a| &a.pane_id == pane_id)
                .map(|a| app.workspace_label(&a.workspace_id))
                .unwrap_or(pane_id.as_str());
            let title = format!("Tags — {label} ({pane_id})");

            let applied: Vec<TagName> = app.store.tags_for(pane_id).into_iter().collect();
            let suggestions = complete::suggest(&app.known, &applied, buffer);

            // Break the sizing circularity: probe `centre` for the width the
            // real box will have -- its horizontal split does not depend on
            // height -- then decide how many suggestion slots the frame's
            // height leaves room for, and only after that compute the real,
            // height-dependent box rect below.
            let width = centre(area, 70, area.height).width.saturating_sub(2);
            let slots = layout::suggestion_slots(area.height);

            let chips = layout::chips(&applied, *tag_cursor, width);
            let chip_line = if chips.visible.is_empty() && chips.omitted == 0 {
                Line::styled("no tags yet", dim)
            } else {
                let mut spans: Vec<Span> = Vec::new();
                for (index, tag) in chips.visible.iter().enumerate() {
                    if index > 0 {
                        spans.push(Span::raw(" "));
                    }
                    let style =
                        if chips.cursor == Some(index) { reversed } else { Style::default() };
                    spans.push(Span::styled(format!("{} ✕", tag.as_str()), style));
                }
                if chips.omitted > 0 {
                    if !chips.visible.is_empty() {
                        spans.push(Span::raw(" "));
                    }
                    spans.push(Span::styled(format!("+{}", chips.omitted), dim));
                }
                Line::from(spans)
            };

            let mut body = vec![chip_line, Line::from(format!("add: {buffer}|"))];
            for (index, tag) in suggestions.iter().take(slots).enumerate() {
                let style = if index == *suggestion_cursor { reversed } else { dim };
                body.push(Line::styled(tag.as_str().to_string(), style));
            }
            body.push(Line::styled("Tab complete · Enter save · ←→ ✕ Backspace · Esc close", dim));

            (title, body, 70)
        }
        Prompt::ConfirmDelete { tag } => (
            "Delete tag everywhere".to_string(),
            vec![
                Line::from(format!("delete `{}` from every agent?", tag.as_str())),
                Line::styled("y or Enter confirm · any other key cancels", dim),
            ],
            60,
        ),
    };

    let height = (body.len() as u16).saturating_add(2).min(area.height);
    let box_area = centre(area, width_percent, height);

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
