use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

pub enum HelpContext {
    Timeline,
    Detail,
    Settings,
}

pub fn draw(frame: &mut Frame, area: Rect, ctx: HelpContext) {
    let lines = match ctx {
        HelpContext::Timeline => timeline_help(),
        HelpContext::Detail => detail_help(),
        HelpContext::Settings => settings_help(),
    };

    let popup = centered_rect(56, lines.len() as u16 + 4, area);
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .title(" Help — press ? or q to close ")
                .title_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        ),
        popup,
    );
}

fn key_line(key: &str, desc: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("  {key:<18}"), Style::default().fg(Color::Yellow)),
        Span::styled(desc.to_string(), Style::default().fg(Color::White)),
    ])
}

fn section(title: &str) -> Line<'static> {
    Line::from(Span::styled(
        format!(" {title}"),
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
    ))
}

fn timeline_help() -> Vec<Line<'static>> {
    vec![
        section("Navigation"),
        key_line("j / ↓", "Select next event"),
        key_line("k / ↑", "Select previous event"),
        key_line("Enter", "Open event detail"),
        key_line("PgUp/PgDn", "Scroll timeline"),
        Line::from(""),
        section("Day"),
        key_line("[", "Previous day"),
        key_line("]", "Next day"),
        key_line("t", "Jump to now (today + scroll to current time)"),
        Line::from(""),
        section("Actions"),
        key_line("o", "Join meeting (Teams app or browser)"),
        key_line("b", "Open event in browser (RSVP, remove, etc.)"),
        key_line("r", "Refresh events"),
        key_line("s", "Settings"),
        key_line("?", "Toggle this help"),
        key_line("q", "Quit"),
    ]
}

fn detail_help() -> Vec<Line<'static>> {
    vec![
        section("Navigation"),
        key_line("↑ / k", "Scroll up"),
        key_line("↓ / j", "Scroll down"),
        key_line("Esc / Backspace", "Back to timeline"),
        Line::from(""),
        section("Actions"),
        key_line("o", "Join meeting (Teams app or browser)"),
        key_line("b", "Open event in browser (to RSVP)"),
        key_line("?", "Toggle this help"),
    ]
}

fn settings_help() -> Vec<Line<'static>> {
    vec![
        section("Navigation"),
        key_line("Tab / ↑↓", "Move between fields"),
        key_line("1–3", "Jump to field"),
        key_line("Space / Enter", "Edit field"),
        key_line("Esc", "Stop editing / back"),
        Line::from(""),
        section("Actions"),
        key_line("g", "Sign in to Google (opens browser)"),
        key_line("n", "Send test notification"),
        key_line("Ctrl+S", "Save config"),
        key_line("?", "Toggle setup guide"),
    ]
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect::new(x, y, width.min(area.width), height.min(area.height))
}
