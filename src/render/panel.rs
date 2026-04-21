use crate::render::signal::signal_span;
use crate::sim::SignalGraph;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Widget};

pub struct SignalPanel<'a> {
    pub graph: &'a SignalGraph,
}

impl<'a> Widget for SignalPanel<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        todo!()
    }
}
