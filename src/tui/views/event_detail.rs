use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};
use ratatui::crossterm::event::{KeyCode, KeyEvent};

use crate::app::{App, AppView};
use crate::calendar::{CalendarEvent, CalendarSource, ResponseStatus};
use super::timeline::source_color;

pub fn draw(_app: &App, event: &CalendarEvent, frame: &mut Frame, area: Rect, scroll: u16) {
    let chunks = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .split(area);

    draw_header(event, frame, chunks[0]);
    draw_body(event, frame, chunks[1], scroll);
    draw_hints(event, frame, chunks[2]);
}

fn draw_header(event: &CalendarEvent, frame: &mut Frame, area: Rect) {
    let source_col = source_color(&event.source);
    let source_label = match event.source {
        CalendarSource::Google => "Google",
        CalendarSource::Microsoft => "Microsoft",
    };
    let rsvp_col = rsvp_color(&event.response_status);
    let rsvp = event.response_status.display();

    let spans = vec![
        Span::styled(format!("[{}] ", source_label), Style::default().fg(source_col)),
        Span::styled(
            event.title.clone(),
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("  ({rsvp})"), Style::default().fg(rsvp_col)),
    ];
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_body(event: &CalendarEvent, frame: &mut Frame, area: Rect, scroll: u16) {
    let mut lines: Vec<Line> = Vec::new();

    let duration = format_duration(event);
    lines.push(Line::from(vec![
        Span::styled("Time:     ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!(
                "{} – {}  ({})",
                event.start.format("%H:%M"),
                event.end.format("%H:%M"),
                duration
            ),
            Style::default().fg(Color::White),
        ),
    ]));

    if let Some(loc) = &event.location {
        lines.push(Line::from(vec![
            Span::styled("Location: ", Style::default().fg(Color::DarkGray)),
            Span::styled(loc.clone(), Style::default().fg(Color::White)),
        ]));
    }

    if let Some(url) = &event.meeting_url {
        let is_teams = url.contains("teams.microsoft.com");
        let is_meet = url.contains("meet.google.com");
        let kind = if is_teams { "Teams" } else if is_meet { "Google Meet" } else { "Video" };
        lines.push(Line::from(vec![
            Span::styled("Meeting:  ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("[{kind}] {url}"),
                Style::default().fg(Color::Cyan).add_modifier(Modifier::UNDERLINED),
            ),
        ]));
        if is_teams {
            let (meeting_id, passcode) = extract_teams_info(
                event.description.as_deref().unwrap_or(""),
            );
            if let Some(id) = meeting_id {
                lines.push(Line::from(vec![
                    Span::styled("Meeting ID:", Style::default().fg(Color::DarkGray)),
                    Span::styled(format!(" {id}"), Style::default().fg(Color::White)),
                ]));
            }
            if let Some(pc) = passcode {
                lines.push(Line::from(vec![
                    Span::styled("Passcode:  ", Style::default().fg(Color::DarkGray)),
                    Span::styled(pc, Style::default().fg(Color::White)),
                ]));
            }
        }
    }

    if let Some(org) = &event.organizer {
        lines.push(Line::from(vec![
            Span::styled("Organizer:", Style::default().fg(Color::DarkGray)),
            Span::styled(format!(" {org}"), Style::default().fg(Color::White)),
        ]));
    }

    lines.push(Line::from(Span::styled(
        "─".repeat(area.width as usize),
        Style::default().fg(Color::DarkGray),
    )));

    if !event.attendees.is_empty() {
        lines.push(Line::from(Span::styled(
            "Attendees:",
            Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD),
        )));
        for att in &event.attendees {
            let name = att.name.as_deref().unwrap_or(&att.email);
            let rsvp_col = rsvp_color(&att.response);
            let rsvp_sym = match att.response {
                ResponseStatus::Accepted => "✓",
                ResponseStatus::Declined => "✗",
                ResponseStatus::Tentative => "?",
                ResponseStatus::NeedsAction => "·",
            };
            let self_mark = if att.is_self { " (you)" } else { "" };
            lines.push(Line::from(vec![
                Span::styled(format!("  {rsvp_sym} "), Style::default().fg(rsvp_col)),
                Span::styled(format!("{name}{self_mark}"), Style::default().fg(Color::White)),
            ]));
        }
        lines.push(Line::from(""));
    }

    if let Some(desc) = &event.description {
        lines.push(Line::from(Span::styled(
            "Description:",
            Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD),
        )));
        let cleaned = strip_html(desc);
        for text_line in cleaned.lines() {
            lines.push(Line::from(Span::styled(
                text_line.to_string(),
                Style::default().fg(Color::White),
            )));
        }
    }

    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().borders(Borders::NONE))
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0)),
        area,
    );
}

fn draw_hints(event: &CalendarEvent, frame: &mut Frame, area: Rect) {
    let mut parts = vec!["[esc] back".to_string()];
    if event.meeting_url.is_some() {
        parts.push("[o] join meeting".to_string());
    }
    if let Some(url) = &event.event_url {
        let label = if url == "https://outlook.office.com/calendar/" {
            "[b] open Outlook (no direct link — navigate manually)"
        } else {
            "[b] open event in browser"
        };
        parts.push(label.to_string());
    }
    parts.push("[↑/↓] scroll".to_string());

    frame.render_widget(
        Paragraph::new(parts.join("  ")).style(Style::default().fg(Color::DarkGray)),
        area,
    );
}

pub fn handle_key(app: &mut App, key: KeyEvent, detail_scroll: &mut u16, _timeline_scroll: &mut i32) {
    let event = match &app.view {
        AppView::EventDetail { event } => event.as_ref().clone(),
        _ => return,
    };

    match key.code {
        KeyCode::Esc | KeyCode::Backspace => {
            app.view = AppView::Timeline;
        }
        KeyCode::Up | KeyCode::Char('k') => {
            *detail_scroll = detail_scroll.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            *detail_scroll = detail_scroll.saturating_add(1);
        }
        KeyCode::Char('o') => {
            join_meeting(&event);
        }
        KeyCode::Char('b') => {
            if let Some(url) = &event.event_url {
                let _ = open::that(url);
            }
        }
        _ => {}
    }
}

fn join_meeting(event: &CalendarEvent) {
    let Some(url) = &event.meeting_url else { return };
    if url.contains("teams.microsoft.com") {
        let teams_uri = url
            .replace("https://teams.microsoft.com", "msteams:")
            .replace("http://teams.microsoft.com", "msteams:");
        if open::that(&teams_uri).is_err() {
            let _ = open::that(url);
        }
    } else {
        let _ = open::that(url);
    }
}

fn rsvp_color(status: &ResponseStatus) -> Color {
    match status {
        ResponseStatus::Accepted => Color::Green,
        ResponseStatus::Declined => Color::Red,
        ResponseStatus::Tentative => Color::Yellow,
        ResponseStatus::NeedsAction => Color::DarkGray,
    }
}

fn format_duration(event: &CalendarEvent) -> String {
    let mins = event.duration_minutes();
    if mins < 60 {
        format!("{mins}m")
    } else {
        let h = mins / 60;
        let m = mins % 60;
        if m == 0 { format!("{h}h") } else { format!("{h}h {m}m") }
    }
}

fn extract_teams_info(description: &str) -> (Option<String>, Option<String>) {
    let mut meeting_id = None;
    let mut passcode = None;
    for line in description.lines() {
        let trimmed = line.trim();
        let lower = trimmed.to_lowercase();
        if lower.starts_with("meeting id:") {
            meeting_id = Some(trimmed["meeting id:".len()..].trim().to_string());
        } else if lower.starts_with("passcode:") {
            passcode = Some(trimmed["passcode:".len()..].trim().to_string());
        }
    }
    (meeting_id, passcode)
}

fn strip_html(s: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    let mut prev_blank = false;
    let mut result = String::new();
    for line in out.lines() {
        let blank = line.trim().is_empty();
        if blank && prev_blank {
            continue;
        }
        result.push_str(line);
        result.push('\n');
        prev_blank = blank;
    }
    result
}
