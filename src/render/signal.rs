use crate::SignalState;
use ratatui::style::{Color, Style};
use ratatui::text::Span;

/// High     -> "━━━  HIGH"   Color::Cyan
/// Low      -> "╌╌╌  LOW"   Color::DarkGray
/// Floating -> "───  FLT"   Color::Gray
/// Conflict -> "▓▓▓  CON"   Color::Red
pub fn signal_span(signal: SignalState) -> Span<'static> {
    match signal {
        SignalState::High => Span::styled("━━━  HIGH", Style::default().fg(Color::Cyan)),
        SignalState::Low => Span::styled("╌╌╌  LOW", Style::default().fg(Color::DarkGray)),
        SignalState::Floating => Span::styled("───  FLT", Style::default().fg(Color::Gray)),
        SignalState::Conflict => Span::styled("▓▓▓  CON", Style::default().fg(Color::Red)),
    }
}
