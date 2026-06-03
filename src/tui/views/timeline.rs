use chrono::{Local, NaiveDate, Timelike};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};
use ratatui::crossterm::event::{KeyCode, KeyEvent};

use crate::app::{App, AppView};
use crate::calendar::{CalendarEvent, CalendarSource, ResponseStatus};

const TIME_COL_W: u16 = 6;
// Minimum rows-per-minute so 10-min meetings always get at least 1 visible row.
// At 0.15 a 30-min meeting gets 4 rows (title + time inside a bordered block).
const MIN_PPM: f32 = 0.15;

pub fn draw(app: &App, frame: &mut Frame, area: Rect, scroll_minutes: i32) {
    let chunks = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .split(area);

    draw_header(app, frame, chunks[0]);
    draw_timeline_body(app, frame, chunks[1], scroll_minutes);
    draw_hints(app, frame, chunks[2]);
}

fn draw_header(app: &App, frame: &mut Frame, area: Rect) {
    let today = Local::now().date_naive();
    let is_today = app.selected_day == today;
    let day_label = format_day_label(app.selected_day, is_today);
    let loading = if app.loading { " [loading…]" } else { "" };
    let err = app.error.as_deref().map(|e| format!("  {e}")).unwrap_or_default();
    let msg = app.status_msg.as_deref().map(|m| format!("  {m}")).unwrap_or_default();

    let mut spans = vec![
        Span::styled(day_label, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
    ];
    if !loading.is_empty() {
        spans.push(Span::styled(loading, Style::default().fg(Color::DarkGray)));
    }
    if !err.is_empty() {
        spans.push(Span::styled(err, Style::default().fg(Color::Red)));
    }
    if !msg.is_empty() {
        spans.push(Span::styled(msg, Style::default().fg(Color::Green)));
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn format_day_label(day: NaiveDate, is_today: bool) -> String {
    let weekday = day.format("%A").to_string();
    let date = day.format("%-d %b %Y").to_string();
    if is_today {
        format!("{weekday} {date} (today)")
    } else {
        format!("{weekday} {date}")
    }
}

fn draw_hints(_app: &App, frame: &mut Frame, area: Rect) {
    let hints = "[j/k] select  [enter] detail  [o] join  [b] open  [[/]] day  [t] now  [r] refresh  [s] settings  [?] help  [q] quit";
    frame.render_widget(
        Paragraph::new(hints).style(Style::default().fg(Color::DarkGray)),
        area,
    );
}

fn draw_timeline_body(app: &App, frame: &mut Frame, area: Rect, scroll_minutes: i32) {
    let start_hour = app.config.display.start_hour as i32;
    let end_hour = app.config.display.end_hour as i32;
    let total_minutes = ((end_hour - start_hour) * 60) as f32;
    let content_h = area.height as f32;
    let ppm = if total_minutes > 0.0 { (content_h / total_minutes).max(MIN_PPM) } else { 1.0 };

    // Clear background
    frame.render_widget(Clear, area);

    // Draw hour grid lines and labels
    for hour in start_hour..=end_hour {
        let mins = (hour - start_hour) * 60;
        let y_virtual = (mins as f32 * ppm) as i32;
        let y_screen = y_virtual - (scroll_minutes as f32 * ppm) as i32;
        if y_screen < 0 || y_screen >= area.height as i32 {
            continue;
        }
        let row = area.y + y_screen as u16;

        // Time label
        frame.render_widget(
            Paragraph::new(format!("{hour:02}:00 "))
                .style(Style::default().fg(Color::DarkGray)),
            Rect::new(area.x, row, TIME_COL_W, 1),
        );

        // Hour separator line
        let line_x = area.x + TIME_COL_W;
        let line_w = area.width.saturating_sub(TIME_COL_W);
        if line_w > 0 {
            frame.render_widget(
                Paragraph::new("─".repeat(line_w as usize))
                    .style(Style::default().fg(Color::Rgb(50, 50, 60))),
                Rect::new(line_x, row, line_w, 1),
            );
        }
    }

    // Current time marker (only when viewing today)
    let today = Local::now().date_naive();
    if app.selected_day == today {
        let now = Local::now();
        let mins_since_start = (now.hour() as i32 - start_hour) * 60 + now.minute() as i32;
        if mins_since_start >= 0 {
            let y_virtual = (mins_since_start as f32 * ppm) as i32;
            let y_screen = y_virtual - (scroll_minutes as f32 * ppm) as i32;
            if y_screen >= 0 && y_screen < area.height as i32 {
                let row = area.y + y_screen as u16;
                let time_str = format!(" {:02}:{:02} ", now.hour(), now.minute());
                let line_x = area.x + TIME_COL_W;
                let line_w = area.width.saturating_sub(TIME_COL_W) as usize;
                let fill = line_w.saturating_sub(time_str.len());
                let half = fill / 2;
                let marker = format!(
                    "{}{}{}",
                    "─".repeat(half),
                    time_str,
                    "─".repeat(fill - half)
                );
                frame.render_widget(
                    Paragraph::new(marker)
                        .style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
                    Rect::new(line_x, row, area.width.saturating_sub(TIME_COL_W), 1),
                );
            }
        }
    }

    // Compute visible non-all-day events
    let events_x = area.x + TIME_COL_W;
    let events_w = area.width.saturating_sub(TIME_COL_W);
    if events_w == 0 {
        return;
    }

    // Separate all-day events
    let timed: Vec<(usize, &CalendarEvent)> = app
        .events
        .iter()
        .enumerate()
        .filter(|(_, e)| !e.is_all_day)
        .collect();

    let groups = compute_overlap_groups(&timed);
    let scroll_rows = (scroll_minutes as f32 * ppm) as i32;

    for group in &groups {
        let n_cols = group.len();
        let col_w = (events_w / n_cols as u16).max(1);

        for (col_idx, &(event_idx, event)) in group.iter().enumerate() {
            let event_start_min =
                (event.start.hour() as i32 - start_hour) * 60 + event.start.minute() as i32;
            let y_virtual = (event_start_min as f32 * ppm) as i32;
            let y_screen = y_virtual - scroll_rows;

            let duration_rows = ((event.duration_minutes() as f32 * ppm) as u16).max(1);

            // Fully out of viewport?
            if y_screen + duration_rows as i32 <= 0 || y_screen >= area.height as i32 {
                continue;
            }

            let clip_top = (-y_screen).max(0) as u16;
            let y_start = (y_screen.max(0) as u16) + area.y;
            let max_h = area.height.saturating_sub(y_start.saturating_sub(area.y));
            let h = duration_rows.saturating_sub(clip_top).min(max_h);
            if h == 0 {
                continue;
            }

            let x = events_x + col_idx as u16 * col_w;
            let w = if col_idx + 1 == n_cols {
                events_w.saturating_sub(col_idx as u16 * col_w)
            } else {
                col_w
            };
            if w == 0 {
                continue;
            }

            let is_selected = event_idx == app.selected_event_idx;
            render_event_block(frame, event, Rect::new(x, y_start, w, h), is_selected, clip_top);
        }
    }

    // All-day events banner at top if any
    let all_day: Vec<&CalendarEvent> = app.events.iter().filter(|e| e.is_all_day).collect();
    if !all_day.is_empty() {
        let banner_h = all_day.len().min(3) as u16;
        let banner = Rect::new(area.x, area.y, area.width, banner_h);
        frame.render_widget(Clear, banner);
        let lines: Vec<Line> = all_day
            .iter()
            .take(3)
            .map(|e| {
                let color = source_color(&e.source);
                Line::from(Span::styled(
                    format!("[all-day] {}", e.title),
                    Style::default().fg(color),
                ))
            })
            .collect();
        frame.render_widget(
            Paragraph::new(lines).block(
                Block::default()
                    .borders(Borders::BOTTOM)
                    .border_style(Style::default().fg(Color::DarkGray)),
            ),
            banner,
        );
    }
}

fn render_event_block(
    frame: &mut Frame,
    event: &CalendarEvent,
    rect: Rect,
    is_selected: bool,
    skip_top: u16,
) {
    // Clear the rect so hour-grid lines behind the event don't show through.
    frame.render_widget(Clear, rect);

    let (border_color, text_color) = event_colors(event, is_selected);
    let bg = if is_selected { Color::Rgb(40, 40, 60) } else { Color::Reset };
    let base_style = Style::default().fg(text_color).bg(bg);
    let brd = Style::default().fg(border_color).bg(bg);
    let needs_action = matches!(event.response_status, ResponseStatus::NeedsAction);
    let cancelled = event.cancelled;

    let prefix = if event.has_meeting_link() { "[M] " } else { "" };
    let time_str = format!("{}-{}", event.start.format("%H:%M"), event.end.format("%H:%M"));
    let loc = event
        .location
        .as_deref()
        .map(|l| format!(" @ {}", truncate(l, 20)))
        .unwrap_or_default();

    // Dashed chars for unanswered invitations; plain chars otherwise.
    let (h_char, v_char) = if needs_action { ("╌", "╎") } else { ("─", "│") };
    let w = rect.width as usize;

    let title_style = if cancelled {
        base_style.add_modifier(Modifier::BOLD | Modifier::CROSSED_OUT)
    } else {
        base_style.add_modifier(Modifier::BOLD)
    };
    let brd = if cancelled { Style::default().fg(Color::DarkGray).bg(bg) } else { brd };

    if rect.height == 1 {
        let inner_w = w.saturating_sub(2);
        let title = truncate(&format!("{}{}", prefix, event.title), inner_w);
        let line = Line::from(vec![
            Span::styled("┌ ", brd),
            Span::styled(title, title_style),
        ]);
        frame.render_widget(Paragraph::new(vec![line]).style(base_style), rect);
    } else if rect.height == 2 {
        let top = format!("┌{}┐", h_char.repeat(w.saturating_sub(2)));
        let inner_w = w.saturating_sub(2);
        let meta = truncate(&format!("{}{} {}{}", prefix, event.title, time_str, loc), inner_w);
        let lines = vec![
            Line::from(Span::styled(top, brd)),
            Line::from(vec![
                Span::styled(format!("{v_char} "), brd),
                Span::styled(meta, title_style),
            ]),
        ];
        frame.render_widget(Paragraph::new(lines).style(base_style), rect);
    } else {
        let inner_w = rect.width.saturating_sub(4) as usize;
        let title = truncate(&format!("{}{}", prefix, event.title), inner_w);
        let mut content: Vec<Line> = vec![
            Line::from(Span::styled(title, title_style)),
        ];
        if rect.height > 3 {
            let meta = truncate(&format!("{}{}", time_str, loc), inner_w);
            content.push(Line::from(Span::styled(meta, base_style)));
        }

        let content_skip = skip_top.saturating_sub(1) as usize;
        let visible: Vec<Line> = content.into_iter().skip(content_skip).collect();

        let border_set = if needs_action {
            symbols::border::Set {
                top_left: "┌",
                top_right: "┐",
                bottom_left: "└",
                bottom_right: "┘",
                vertical_left: "╎",
                vertical_right: "╎",
                horizontal_top: "╌",
                horizontal_bottom: "╌",
            }
        } else {
            symbols::border::PLAIN
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_set(border_set)
            .border_style(if is_selected { brd.add_modifier(Modifier::BOLD) } else { brd });

        frame.render_widget(Paragraph::new(visible).block(block).style(base_style), rect);
    }
}

fn event_colors(event: &CalendarEvent, is_selected: bool) -> (Color, Color) {
    let now = Local::now();
    let is_past = event.start < now;
    let source_col = source_color(&event.source);

    let border = match &event.response_status {
        ResponseStatus::Accepted | ResponseStatus::NeedsAction => {
            if is_past { Color::DarkGray } else { source_col }
        }
        ResponseStatus::Declined => Color::DarkGray,
        ResponseStatus::Tentative => Color::Yellow,
    };

    let border = if is_selected { source_col } else { border };
    (border, Color::White)
}

pub fn source_color(source: &CalendarSource) -> Color {
    match source {
        CalendarSource::Google => Color::Green,
        CalendarSource::Microsoft => Color::Cyan,
    }
}

fn compute_overlap_groups<'a>(
    events: &[(usize, &'a CalendarEvent)],
) -> Vec<Vec<(usize, &'a CalendarEvent)>> {
    if events.is_empty() {
        return Vec::new();
    }

    let mut groups: Vec<Vec<(usize, &CalendarEvent)>> = Vec::new();
    let mut current: Vec<(usize, &CalendarEvent)> = Vec::new();
    let mut max_end = events[0].1.end;

    for &(idx, event) in events {
        if current.is_empty() || event.start < max_end {
            if event.end > max_end {
                max_end = event.end;
            }
            current.push((idx, event));
        } else {
            groups.push(current.clone());
            current = vec![(idx, event)];
            max_end = event.end;
        }
    }
    if !current.is_empty() {
        groups.push(current);
    }
    groups
}

fn scroll_to_show_selected(app: &App, scroll_minutes: &mut i32, body_h: u16) {
    let Some(event) = app.selected_event() else { return };
    if event.is_all_day || body_h == 0 { return; }

    let start_hour = app.config.display.start_hour as i32;
    let end_hour = app.config.display.end_hour as i32;
    let total_minutes = ((end_hour - start_hour) * 60) as f32;
    let ppm = (body_h as f32 / total_minutes).max(MIN_PPM);

    let event_start_min =
        (event.start.hour() as i32 - start_hour) * 60 + event.start.minute() as i32;
    let event_end_min = event_start_min + event.duration_minutes() as i32;

    let y_top = (event_start_min as f32 * ppm) as i32;
    let y_bot = (event_end_min as f32 * ppm) as i32;

    let scroll_px = (*scroll_minutes as f32 * ppm) as i32;
    let view_bot = scroll_px + body_h as i32;

    if y_top < scroll_px {
        *scroll_minutes = (y_top as f32 / ppm) as i32;
    } else if y_bot > view_bot {
        *scroll_minutes = ((y_bot - body_h as i32) as f32 / ppm).ceil() as i32;
    }
    *scroll_minutes = (*scroll_minutes).max(0);
}

fn truncate(s: &str, max: usize) -> String {
    if max < 4 {
        return s.chars().take(max).collect();
    }
    if s.chars().count() > max {
        format!("{}…", s.chars().take(max - 1).collect::<String>())
    } else {
        s.to_string()
    }
}

pub fn handle_key(
    app: &mut App,
    key: KeyEvent,
    scroll_minutes: &mut i32,
    detail_scroll: &mut u16,
    term_h: u16,
) {
    // Header row + hints row = 2; remaining rows are the timeline body.
    let body_h = term_h.saturating_sub(2);
    match key.code {
        KeyCode::Char('q') => app.should_quit = true,
        KeyCode::Char('j') | KeyCode::Down => {
            app.navigate_down();
            scroll_to_show_selected(app, scroll_minutes, body_h);
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.navigate_up();
            scroll_to_show_selected(app, scroll_minutes, body_h);
        }
        KeyCode::Enter => {
            if let Some(event) = app.selected_event() {
                *detail_scroll = 0;
                app.view = AppView::EventDetail {
                    event: Box::new(event.clone()),
                };
            }
        }
        KeyCode::Char('[') => {
            app.prev_day();
            *scroll_minutes = 0;
        }
        KeyCode::Char(']') => {
            app.next_day();
            *scroll_minutes = 0;
        }
        KeyCode::Char('t') => app.go_today(),
        KeyCode::Char('o') => {
            if let Some(event) = app.selected_event() {
                if let Some(url) = event.meeting_url.clone() {
                    if url.contains("teams.microsoft.com") {
                        let teams_uri = url
                            .replace("https://teams.microsoft.com", "msteams:")
                            .replace("http://teams.microsoft.com", "msteams:");
                        if open::that(&teams_uri).is_err() {
                            let _ = open::that(&url);
                        }
                    } else {
                        let _ = open::that(&url);
                    }
                }
            }
        }
        KeyCode::Char('b') => {
            if let Some(url) = app.selected_event().and_then(|e| e.event_url.clone()) {
                let _ = open::that(url);
            }
        }
        KeyCode::Char('r') => app.trigger_load(),
        KeyCode::Char('s') => {
            app.view = AppView::Settings;
        }
        KeyCode::PageUp => *scroll_minutes = (*scroll_minutes - 60).max(0),
        KeyCode::PageDown => *scroll_minutes += 60,
        _ => {}
    }
}
