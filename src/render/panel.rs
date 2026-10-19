use crate::render::signal::signal_span;
use crate::sim::transistor::{SignalGraph, SignalId};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Widget};

pub struct SignalPanel<'a> {
    pub graph: &'a SignalGraph,
    /// inputs get their toggle key (1-9) printed next to them
    pub inputs: &'a [SignalId],
}

impl<'a> Widget for SignalPanel<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(Span::styled(
                " SIGNALS ",
                Style::default().fg(Color::Rgb(255, 200, 0)),
            ))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Rgb(20, 60, 20)));
        let inner = block.inner(area);
        block.render(area, buf);

        let key_style = Style::default().fg(Color::Rgb(96, 136, 96));
        let name_style = Style::default().fg(Color::Rgb(255, 200, 0));
        // rows are counted over named signals only, so anonymous internal nets
        // don't leave gaps or push named ones off the panel
        self.graph
            .signals
            .iter()
            .zip(self.graph.signal_names.iter())
            .enumerate()
            .filter_map(|(i, (state, name))| name.as_deref().map(|n| (i, n, *state)))
            .take(inner.height as usize)
            .enumerate()
            .for_each(|(row, (i, name, state))| {
                let key = match self.inputs.iter().position(|s| s.0 == i) {
                    Some(k) if k < 9 => format!("{} ", k + 1),
                    _ => "  ".to_string(),
                };
                let line = Line::from(vec![
                    Span::styled(key, key_style),
                    Span::styled(format!("{:<6}", name), name_style),
                    signal_span(state),
                ]);
                buf.set_line(inner.x, inner.y + row as u16, &line, inner.width);
            });
    }
}
