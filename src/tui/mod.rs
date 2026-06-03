pub mod views;

use std::io;
use std::sync::Arc;
use std::time::Duration;
use anyhow::Result;
use chrono::{Local, Timelike};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use ratatui::crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers, MouseEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use tokio::sync::{mpsc, RwLock};

use crate::app::{App, AppEvent, AppView};
use crate::calendar::CalendarEvent;
use crate::config::{save_config, Config, NotificationConfig};
use crate::notification::spawn_notification_watcher;
use views::{event_detail, help, settings, timeline};

pub async fn run_tui(config: Config) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(&mut terminal, config).await;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;
    result
}

async fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, config: Config) -> Result<()> {
    let (event_tx, mut event_rx) = mpsc::channel::<AppEvent>(64);
    let mut app = App::new(config, event_tx.clone());

    let shared_events: Arc<RwLock<Vec<CalendarEvent>>> = Arc::new(RwLock::new(Vec::new()));
    let shared_notif_config: Arc<RwLock<NotificationConfig>> =
        Arc::new(RwLock::new(app.config.notifications.clone()));
    spawn_notification_watcher(event_tx.clone(), shared_events.clone(), shared_notif_config.clone());

    if app.has_any_calendar() {
        app.trigger_load();
    } else {
        app.view = AppView::Settings;
        app.status_msg = Some("Paste your calendar ICS URL to get started — press ? for help".to_string());
    }

    let mut settings_state = settings::SettingsState::new(&app.config);
    let mut detail_scroll: u16 = 0;
    let mut timeline_scroll: i32 = default_timeline_scroll(&app);

    loop {
        if app.should_quit {
            break;
        }

        {
            let mut guard = shared_events.write().await;
            *guard = app.events.clone();
        }
        {
            let mut guard = shared_notif_config.write().await;
            *guard = app.config.notifications.clone();
        }

        terminal.draw(|frame| {
            let area = frame.area();
            match &app.view {
                AppView::Timeline => timeline::draw(&app, frame, area, timeline_scroll),
                AppView::EventDetail { event } => event_detail::draw(&app, event, frame, area, detail_scroll),
                AppView::Settings => settings::draw(&app, &mut settings_state, frame, area),
            }
            if app.show_help {
                let ctx = match &app.view {
                    AppView::Timeline => help::HelpContext::Timeline,
                    AppView::EventDetail { .. } => help::HelpContext::Detail,
                    AppView::Settings => help::HelpContext::Settings,
                };
                help::draw(frame, area, ctx);
            }
        })?;

        tokio::select! {
            Some(app_event) = event_rx.recv() => {
                if let AppEvent::AuthCompleted(ref refresh_token) = app_event {
                    let mut new_config = (*app.config).clone();
                    new_config.google.refresh_token = refresh_token.clone();
                    if save_config(&new_config).is_ok() {
                        app.config = Arc::new(new_config);
                        app.rebuild_clients();
                        app.status_msg = Some("Google authenticated — loading events…".to_string());
                        app.trigger_load();
                    }
                }
                app.handle_event(app_event);
                if app.scroll_to_now {
                    timeline_scroll = default_timeline_scroll(&app);
                    app.scroll_to_now = false;
                }
            }

            poll_result = tokio::task::spawn_blocking(|| event::poll(Duration::from_millis(50))) => {
                if let Ok(Ok(true)) = poll_result {
                    match event::read() {
                        Ok(Event::Key(key)) if key.kind == KeyEventKind::Press => {
                            app.error = None;
                            app.status_msg = None;

                            if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                                app.should_quit = true;
                                continue;
                            }

                            if key.code == KeyCode::Char('?') && !settings_state.editing {
                                if matches!(app.view, AppView::Settings) {
                                    settings_state.show_guide = !settings_state.show_guide;
                                    settings_state.guide_scroll = 0;
                                } else {
                                    app.show_help = !app.show_help;
                                }
                                continue;
                            }
                            if settings_state.show_guide {
                                match key.code {
                                    KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q') => {
                                        settings_state.show_guide = false;
                                    }
                                    KeyCode::Up | KeyCode::Char('k') => {
                                        settings_state.guide_scroll = settings_state.guide_scroll.saturating_sub(1);
                                    }
                                    KeyCode::Down | KeyCode::Char('j') => {
                                        settings_state.guide_scroll = settings_state.guide_scroll.saturating_add(1);
                                    }
                                    _ => {}
                                }
                                continue;
                            }
                            if app.show_help {
                                if matches!(key.code, KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q')) {
                                    app.show_help = false;
                                }
                                continue;
                            }

                            let term_h = terminal.size().map(|r| r.height).unwrap_or(24);
                            match &app.view {
                                AppView::Timeline => {
                                    timeline::handle_key(&mut app, key, &mut timeline_scroll, &mut detail_scroll, term_h);
                                }
                                AppView::EventDetail { .. } => {
                                    event_detail::handle_key(&mut app, key, &mut detail_scroll, &mut timeline_scroll);
                                }
                                AppView::Settings => {
                                    settings::handle_key(&mut app, &mut settings_state, key);
                                }
                            }
                        }
                        Ok(Event::Mouse(mouse)) => {
                            match mouse.kind {
                                MouseEventKind::ScrollUp => {
                                    if settings_state.show_guide {
                                        settings_state.guide_scroll = settings_state.guide_scroll.saturating_sub(2);
                                    } else {
                                        match &app.view {
                                            AppView::Timeline => timeline_scroll = (timeline_scroll - 15).max(0),
                                            AppView::EventDetail { .. } => detail_scroll = detail_scroll.saturating_sub(2),
                                            _ => {}
                                        }
                                    }
                                }
                                MouseEventKind::ScrollDown => {
                                    if settings_state.show_guide {
                                        settings_state.guide_scroll = settings_state.guide_scroll.saturating_add(2);
                                    } else {
                                        match &app.view {
                                            AppView::Timeline => timeline_scroll += 15,
                                            AppView::EventDetail { .. } => detail_scroll = detail_scroll.saturating_add(2),
                                            _ => {}
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    Ok(())
}

fn default_timeline_scroll(app: &App) -> i32 {
    let today = Local::now().date_naive();
    if app.selected_day == today {
        let now = Local::now();
        let start_hour = app.config.display.start_hour as i32;
        let mins_since_start = (now.hour() as i32 - start_hour) * 60 + now.minute() as i32;
        (mins_since_start - 30).max(0)
    } else {
        0
    }
}
