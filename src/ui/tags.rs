use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

use super::{overlay, App};
use crate::model::Mode;

/// `mode glyph · tag name · count`.
pub fn render(frame: &mut ratatui::Frame, area: Rect, app: &App) {
    let dim = Style::default().add_modifier(Modifier::DIM);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!("Tags ({})", app.known.len()));

    if app.known.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::styled(
                "no tags yet — press 1, pick an agent, press a",
                dim,
            ))
            .block(block),
            area,
        );
        overlay::render(frame, area, app);
        return;
    }

    let items: Vec<ListItem> = app
        .known
        .iter()
        .enumerate()
        .map(|(index, tag)| {
            let (glyph, style) = match app.filter.mode(tag) {
                Mode::In => ("+", Style::default().fg(Color::Green)),
                Mode::Out => ("−", Style::default().fg(Color::Red)),
                Mode::Off => ("·", dim),
            };
            let count = app.counts.get(tag).copied().unwrap_or(0);
            let line = Line::from(vec![
                Span::styled(format!("{glyph} "), style),
                Span::raw(format!("{:<28} ", tag.as_str())),
                Span::styled(count.to_string(), dim),
            ]);
            let item = ListItem::new(line);
            if index == app.tag_cursor {
                item.style(Style::default().add_modifier(Modifier::REVERSED))
            } else {
                item
            }
        })
        .collect();

    frame.render_widget(List::new(items).block(block), area);
    overlay::render(frame, area, app);
}
