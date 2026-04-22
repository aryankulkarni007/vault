use crate::render;
use crate::render::signal::{self, signal_span};
use crate::sim::SignalGraph;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Widget};

pub struct SignalPanel<'a> {
    pub graph: &'a SignalGraph,
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
        self.graph
            .signals
            .iter()
            .zip(self.graph.signal_names.iter())
            .enumerate()
            .for_each(|(i, (state, name))| {
                if name.is_none() {
                    return;
                }
                let y = inner.y + i as u16;
                if y >= inner.y + inner.height {
                    return;
                }
                let name_str = name.as_deref().unwrap_or("???");
                let line = Line::from(vec![
                    Span::styled(
                        format!("{:<8}", name_str),
                        Style::default().fg(Color::Rgb(255, 200, 0)),
                    ),
                    signal_span(*state),
                ]);
                buf.set_line(inner.x, y, &line, inner.width);
            });
    }
}
