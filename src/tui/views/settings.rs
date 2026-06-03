use std::sync::Arc;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tui_textarea::TextArea;

use crate::app::{App, AppEvent, AppView};
use crate::calendar::google::GoogleClient;
use crate::config::{Config, save_config};

const F_G_CLIENT_ID: usize = 0;
const F_G_CLIENT_SECRET: usize = 1;
const F_MS_ICS_URL: usize = 2;
const F_NOTIFY_BEFORE: usize = 3;
const NUM_TEXT_FIELDS: usize = 4;

const NAV_NOTIFY_ON_START: usize = 4;
const NUM_NAV: usize = 5;

pub struct SettingsState {
    pub fields: [TextArea<'static>; NUM_TEXT_FIELDS],
    pub active: usize,
    pub editing: bool,
    pub show_guide: bool,
    pub guide_scroll: u16,
    pub notify_on_start: bool,
}

impl SettingsState {
    pub fn new(config: &Config) -> Self {
        let make = |val: &str| {
            let mut ta = TextArea::default();
            ta.insert_str(val);
            ta
        };
        let before_str = config.notifications.notify_before_minutes
            .map(|m| m.to_string())
            .unwrap_or_default();
        Self {
            fields: [
                make(&config.google.client_id),
                make(&config.google.client_secret),
                make(&config.microsoft.ics_url),
                make(&before_str),
            ],
            active: 0,
            editing: false,
            show_guide: false,
            guide_scroll: 0,
            notify_on_start: config.notifications.notify_on_start,
        }
    }

    fn first_line(&self, idx: usize) -> String {
        self.fields[idx].lines().first().map(|s| s.trim().to_string()).unwrap_or_default()
    }

    pub fn build_config(&self, base: &Config) -> Config {
        let mut c = base.clone();
        c.google.client_id = self.first_line(F_G_CLIENT_ID);
        c.google.client_secret = self.first_line(F_G_CLIENT_SECRET);
        c.microsoft.ics_url = self.first_line(F_MS_ICS_URL);
        let before_str = self.first_line(F_NOTIFY_BEFORE);
        c.notifications.notify_before_minutes = if before_str.is_empty() {
            None
        } else {
            before_str.parse::<u64>().ok()
        };
        c.notifications.notify_on_start = self.notify_on_start;
        c
    }
}

pub fn draw(app: &App, state: &mut SettingsState, frame: &mut Frame, area: Rect) {
    let chunks = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .split(area);

    let header = Line::from(vec![
        Span::styled("Settings", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::styled("  — configure calendar sources", Style::default().fg(Color::DarkGray)),
    ]);
    frame.render_widget(Paragraph::new(header), chunks[0]);

    let hints = if state.editing {
        "[esc] stop editing  [ctrl+s] save"
    } else {
        "[tab/↑↓] navigate  [space/enter] edit/toggle  [g] sign in to Google  [n] test notification  [ctrl+s] save  [?] guide  [esc] back"
    };
    frame.render_widget(
        Paragraph::new(hints).style(Style::default().fg(Color::DarkGray)),
        chunks[2],
    );

    let body = chunks[1];
    let fh = 3u16;
    let layout = Layout::vertical([
        Constraint::Length(1),   // [0]  Google header
        Constraint::Length(fh),  // [1]  client_id
        Constraint::Length(fh),  // [2]  client_secret
        Constraint::Length(1),   // [3]  auth status
        Constraint::Length(1),   // [4]  spacer
        Constraint::Length(1),   // [5]  Microsoft header
        Constraint::Length(fh),  // [6]  ICS URL
        Constraint::Length(1),   // [7]  ICS status
        Constraint::Length(1),   // [8]  spacer
        Constraint::Length(1),   // [9]  Notifications header
        Constraint::Length(fh),  // [10] notify before field
        Constraint::Length(fh),  // [11] notify on start toggle
        Constraint::Min(0),      // [12] messages
    ])
    .split(body);

    // ── Google ──
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "Google Calendar",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ))),
        layout[0],
    );
    render_field(frame, state, F_G_CLIENT_ID, "1. Client ID", false, layout[1]);
    render_field(frame, state, F_G_CLIENT_SECRET, "2. Client Secret", true, layout[2]);
    frame.render_widget(
        Paragraph::new(Line::from(if !app.config.google.refresh_token.is_empty() {
            Span::styled("   ✓ Authenticated — [g] to re-authenticate", Style::default().fg(Color::Green))
        } else if !app.config.google.client_id.is_empty() {
            Span::styled("   Not signed in — press [g] to open browser sign-in", Style::default().fg(Color::Yellow))
        } else {
            Span::styled("   Enter Client ID + Secret, then press [g]", Style::default().fg(Color::DarkGray))
        })),
        layout[3],
    );

    frame.render_widget(Paragraph::new(""), layout[4]);

    // ── Microsoft ──
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "Microsoft / Teams",
            Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD),
        ))),
        layout[5],
    );
    render_field(frame, state, F_MS_ICS_URL, "3. ICS URL", false, layout[6]);
    frame.render_widget(
        Paragraph::new(Line::from(if !app.config.microsoft.ics_url.is_empty() {
            Span::styled("   ✓ ICS URL set", Style::default().fg(Color::Green))
        } else {
            Span::styled("   Not set — see ? guide", Style::default().fg(Color::DarkGray))
        })),
        layout[7],
    );

    frame.render_widget(Paragraph::new(""), layout[8]);

    // ── Notifications ──
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "Notifications",
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        ))),
        layout[9],
    );
    render_field(frame, state, F_NOTIFY_BEFORE, "4. Minutes before meeting (blank = off)", false, layout[10]);
    render_toggle(frame, state, "5. Notify on meeting start", layout[11]);

    if let Some(err) = &app.error {
        frame.render_widget(
            Paragraph::new(err.clone()).style(Style::default().fg(Color::Red)),
            layout[12],
        );
    } else if let Some(msg) = &app.status_msg {
        frame.render_widget(
            Paragraph::new(msg.clone()).style(Style::default().fg(Color::Green)),
            layout[12],
        );
    }

    if state.show_guide {
        draw_guide(frame, area, state.guide_scroll);
    }
}

fn render_field(
    frame: &mut Frame,
    state: &mut SettingsState,
    idx: usize,
    label: &str,
    secret: bool,
    area: Rect,
) {
    let is_active = state.active == idx;
    let is_editing = is_active && state.editing;

    let border_color = if is_editing {
        Color::Yellow
    } else if is_active {
        Color::Cyan
    } else {
        Color::DarkGray
    };

    let block = Block::default()
        .title(format!(" {label} "))
        .title_style(Style::default().fg(if is_active { Color::White } else { Color::DarkGray }))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    if is_editing {
        state.fields[idx].set_block(block);
        frame.render_widget(&state.fields[idx], area);
    } else {
        let val = state.fields[idx].lines().first().cloned().unwrap_or_default();
        let display = if secret && !val.is_empty() {
            "•".repeat(val.len().min(40))
        } else {
            val
        };
        frame.render_widget(Paragraph::new(display).block(block), area);
    }
}

fn render_toggle(frame: &mut Frame, state: &SettingsState, label: &str, area: Rect) {
    let is_active = state.active == NAV_NOTIFY_ON_START;
    let border_color = if is_active { Color::Cyan } else { Color::DarkGray };
    let block = Block::default()
        .title(format!(" {label} "))
        .title_style(Style::default().fg(if is_active { Color::White } else { Color::DarkGray }))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    let (text, color) = if state.notify_on_start {
        ("● On", Color::Green)
    } else {
        ("○ Off", Color::DarkGray)
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(text, Style::default().fg(color)))).block(block),
        area,
    );
}

fn draw_guide(frame: &mut Frame, area: Rect, scroll: u16) {
    let popup = centered_rect(72, area.height.saturating_sub(4), area);
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(guide_lines())
            .block(
                Block::default()
                    .title(" Setup Guide — [↑/↓] scroll  [?/esc] close ")
                    .title_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Yellow)),
            )
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0)),
        popup,
    );
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect::new(x, y, width.min(area.width), height.min(area.height))
}

fn s(label: &'static str, color: Color) -> Line<'static> {
    Line::from(Span::styled(label, Style::default().fg(color).add_modifier(Modifier::BOLD)))
}

fn step(n: &'static str, text: &'static str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("  {n}. "), Style::default().fg(Color::Yellow)),
        Span::styled(text, Style::default().fg(Color::White)),
    ])
}

fn note(text: &'static str) -> Line<'static> {
    Line::from(Span::styled(
        format!("     {text}"),
        Style::default().fg(Color::DarkGray),
    ))
}

fn blank() -> Line<'static> { Line::from("") }

fn guide_lines() -> Vec<Line<'static>> {
    vec![
        blank(),
        s("  Google Calendar — OAuth setup", Color::Cyan),
        blank(),
        step("1", "Create a Google Cloud project (free)"),
        note("Go to console.cloud.google.com → New project"),
        blank(),
        step("2", "Enable the Google Calendar API"),
        note("APIs & Services → Library → search \"Google Calendar API\" → Enable"),
        blank(),
        step("3", "Create OAuth credentials"),
        note("APIs & Services → Credentials → + Create Credentials → OAuth client ID"),
        note("Configure consent screen if prompted (External, test mode is fine)"),
        note("Application type: Desktop app — give it any name"),
        note("Click Create — copy the Client ID and Client Secret"),
        blank(),
        step("4", "Paste Client ID into field 1 and Client Secret into field 2 above"),
        note("Press Ctrl+S to save, then press [g] to open browser sign-in"),
        blank(),
        step("5", "Sign in to Google in the browser that opens"),
        note("After sign-in you'll see \"Authenticated!\" — return to caltui"),
        note("Your refresh token is saved automatically"),
        blank(),
        blank(),
        s("  Microsoft / Teams — ICS setup (no sign-in needed)", Color::Blue),
        blank(),
        step("1", "Open Outlook calendar in your browser"),
        note("outlook.live.com/calendar  or  outlook.office.com"),
        blank(),
        step("2", "Right-click your calendar → Share → Publish to a calendar"),
        note("(Newer Outlook: Settings → View all settings → Calendar → Shared calendars)"),
        blank(),
        step("3", "Set permissions to \"Can view all details\", click Publish"),
        blank(),
        step("4", "Copy the ICS link and paste into field 3, press Ctrl+S"),
        note("If you received a calendar sharing invitation, the ICS URL is in that email."),
        blank(),
        blank(),
        s("  Responding to invitations", Color::Yellow),
        blank(),
        note("In event detail, press [b] to open the event in your browser and RSVP there."),
        blank(),
    ]
}

pub fn handle_key(app: &mut App, state: &mut SettingsState, key: KeyEvent) {
    if state.editing {
        match key.code {
            KeyCode::Esc => state.editing = false,
            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                state.editing = false;
                save_settings(app, state);
            }
            _ => {
                if state.active < NUM_TEXT_FIELDS {
                    state.fields[state.active].input(key);
                }
            }
        }
        return;
    }

    match key.code {
        KeyCode::Esc | KeyCode::Backspace => app.view = AppView::Timeline,
        KeyCode::Tab | KeyCode::Down | KeyCode::Char('j') => {
            state.active = (state.active + 1) % NUM_NAV;
        }
        KeyCode::BackTab | KeyCode::Up | KeyCode::Char('k') => {
            state.active = state.active.checked_sub(1).unwrap_or(NUM_NAV - 1);
        }
        KeyCode::Char('1') => state.active = F_G_CLIENT_ID,
        KeyCode::Char('2') => state.active = F_G_CLIENT_SECRET,
        KeyCode::Char('3') => state.active = F_MS_ICS_URL,
        KeyCode::Char('4') => state.active = F_NOTIFY_BEFORE,
        KeyCode::Char('5') => state.active = NAV_NOTIFY_ON_START,
        KeyCode::Enter | KeyCode::Char(' ') => {
            if state.active == NAV_NOTIFY_ON_START {
                state.notify_on_start = !state.notify_on_start;
            } else {
                state.editing = true;
            }
        }
        KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            save_settings(app, state);
        }
        KeyCode::Char('g') => {
            start_google_auth(app, state);
        }
        KeyCode::Char('n') => {
            notify_rust::Notification::new()
                .summary("caltui — test notification")
                .body("Notifications are working correctly")
                .timeout(notify_rust::Timeout::Milliseconds(4000))
                .show()
                .ok();
            app.status_msg = Some("Test notification sent".to_string());
        }
        _ => {}
    }
}

fn save_settings(app: &mut App, state: &SettingsState) {
    let new_config = state.build_config(&app.config);
    match save_config(&new_config) {
        Ok(()) => {
            app.config = Arc::new(new_config);
            app.rebuild_clients();
            if app.has_any_calendar() {
                app.trigger_load();
            }
            app.status_msg = Some("Config saved".to_string());
        }
        Err(e) => {
            app.error = Some(format!("Save failed: {e}"));
        }
    }
}

fn start_google_auth(app: &mut App, state: &SettingsState) {
    let client_id = state.first_line(F_G_CLIENT_ID);
    let client_secret = state.first_line(F_G_CLIENT_SECRET);

    if client_id.is_empty() || client_secret.is_empty() {
        app.error = Some("Enter Client ID and Client Secret first".to_string());
        return;
    }

    let mut tmp = (*app.config).clone();
    tmp.google.client_id = client_id;
    tmp.google.client_secret = client_secret;
    let cfg = Arc::new(tmp);
    let client = GoogleClient::new(cfg);
    let tx = app.event_tx.clone();

    tokio::spawn(async move {
        match client.start_oauth_flow().await {
            Ok(refresh_token) => {
                let _ = tx.send(AppEvent::AuthCompleted(refresh_token)).await;
            }
            Err(e) => {
                let _ = tx.send(AppEvent::Error(format!("Google auth failed: {e}"))).await;
            }
        }
    });

    app.status_msg = Some("Browser opened — sign in to Google, then return here…".to_string());
}
