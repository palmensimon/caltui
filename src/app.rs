use std::sync::Arc;
use chrono::{Local, NaiveDate};
use tokio::sync::mpsc;

use crate::calendar::CalendarEvent;
use crate::calendar::google::GoogleClient;
use crate::calendar::ics::IcsClient;
use crate::calendar::vdir::VdirClient;
use crate::calendar::CalendarSource;
use crate::config::Config;

#[derive(Debug)]
pub enum AppView {
    Timeline,
    EventDetail { event: Box<CalendarEvent> },
    Settings,
}

#[derive(Debug)]
pub enum AppEvent {
    EventsLoaded(Vec<CalendarEvent>),
    AuthCompleted(String), // refresh_token
    Tick,
    Error(String),
}

pub struct App {
    pub view: AppView,
    pub events: Vec<CalendarEvent>,
    pub selected_day: NaiveDate,
    pub selected_event_idx: usize,
    pub loading: bool,
    pub error: Option<String>,
    pub status_msg: Option<String>,
    pub show_help: bool,
    pub should_quit: bool,
    pub scroll_to_now: bool,
    pub config: Arc<Config>,
    pub google_client: Option<Arc<GoogleClient>>,
    pub google_vdir: Option<Arc<VdirClient>>,
    pub google_ics: Option<Arc<IcsClient>>,
    pub ms_ics: Option<Arc<IcsClient>>,
    pub event_tx: mpsc::Sender<AppEvent>,
}

impl App {
    pub fn new(config: Config, event_tx: mpsc::Sender<AppEvent>) -> Self {
        let config = Arc::new(config);
        let (google_client, google_vdir, google_ics, ms_ics) = build_clients(config.clone());
        Self {
            view: AppView::Timeline,
            events: Vec::new(),
            selected_day: Local::now().date_naive(),
            selected_event_idx: 0,
            loading: false,
            error: None,
            status_msg: None,
            show_help: false,
            should_quit: false,
            scroll_to_now: false,
            config,
            google_client,
            google_vdir,
            google_ics,
            ms_ics,
            event_tx,
        }
    }

    pub fn has_any_calendar(&self) -> bool {
        self.google_client.is_some()
            || self.google_vdir.is_some()
            || self.google_ics.is_some()
            || self.ms_ics.is_some()
    }

    pub fn handle_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::EventsLoaded(mut new_events) => {
                self.loading = false;
                new_events.sort_by_key(|e| e.start);
                self.events = new_events;
                self.selected_event_idx =
                    self.selected_event_idx.min(self.events.len().saturating_sub(1));
            }
            AppEvent::AuthCompleted(_) | AppEvent::Tick => {}
            AppEvent::Error(e) => self.error = Some(e),
        }
    }

    pub fn trigger_load(&mut self) {
        if !self.has_any_calendar() {
            return;
        }
        self.loading = true;
        self.error = None;
        let day = self.selected_day;
        let tx = self.event_tx.clone();
        let google_client = self.google_client.clone();
        let google_vdir = self.google_vdir.clone();
        let google_ics = self.google_ics.clone();
        let ms_ics = self.ms_ics.clone();

        tokio::spawn(async move {
            let mut all: Vec<CalendarEvent> = Vec::new();
            let mut errors: Vec<String> = Vec::new();

            if let Some(g) = google_client {
                match g.list_events(day).await {
                    Ok(evs) => all.extend(evs),
                    Err(e) => errors.push(format!("Google: {e}")),
                }
            } else if let Some(g) = google_vdir {
                match g.list_events(day).await {
                    Ok(evs) => all.extend(evs),
                    Err(e) => errors.push(format!("Google (vdir): {e}")),
                }
            } else if let Some(g) = google_ics {
                match g.list_events(day).await {
                    Ok(evs) => all.extend(evs),
                    Err(e) => errors.push(format!("Google: {e}")),
                }
            }

            if let Some(m) = ms_ics {
                match m.list_events(day).await {
                    Ok(evs) => all.extend(evs),
                    Err(e) => errors.push(format!("Microsoft: {e}")),
                }
            }

            if !errors.is_empty() {
                let _ = tx.send(AppEvent::Error(errors.join("; "))).await;
            }
            let _ = tx.send(AppEvent::EventsLoaded(all)).await;
        });
    }

    pub fn rebuild_clients(&mut self) {
        let (gc, gv, gi, ms) = build_clients(self.config.clone());
        self.google_client = gc;
        self.google_vdir = gv;
        self.google_ics = gi;
        self.ms_ics = ms;
    }

    pub fn selected_event(&self) -> Option<&CalendarEvent> {
        self.events.get(self.selected_event_idx)
    }

    pub fn navigate_up(&mut self) {
        if self.selected_event_idx > 0 {
            self.selected_event_idx -= 1;
        }
    }

    pub fn navigate_down(&mut self) {
        if self.selected_event_idx + 1 < self.events.len() {
            self.selected_event_idx += 1;
        }
    }

    pub fn next_day(&mut self) {
        self.selected_day = self.selected_day.succ_opt().unwrap_or(self.selected_day);
        self.selected_event_idx = 0;
        self.events.clear();
        self.trigger_load();
    }

    pub fn prev_day(&mut self) {
        self.selected_day = self.selected_day.pred_opt().unwrap_or(self.selected_day);
        self.selected_event_idx = 0;
        self.events.clear();
        self.trigger_load();
    }

    pub fn go_today(&mut self) {
        let today = Local::now().date_naive();
        if self.selected_day != today {
            self.selected_day = today;
            self.events.clear();
            self.trigger_load();
        }
        self.selected_event_idx = 0;
        self.scroll_to_now = true;
    }
}

fn build_clients(
    config: Arc<Config>,
) -> (
    Option<Arc<GoogleClient>>,
    Option<Arc<VdirClient>>,
    Option<Arc<IcsClient>>,
    Option<Arc<IcsClient>>,
) {
    // OAuth takes priority
    let google_client = if !config.google.client_id.is_empty()
        && !config.google.refresh_token.is_empty()
    {
        Some(Arc::new(GoogleClient::new(config.clone())))
    } else {
        None
    };

    // vdir fallback
    let google_vdir = if google_client.is_none() && !config.google.vdir_path.is_empty() {
        Some(Arc::new(VdirClient::new(
            config.google.vdir_path.clone(),
            CalendarSource::Google,
        )))
    } else {
        None
    };

    // ICS fallback
    let google_ics =
        if google_client.is_none() && google_vdir.is_none() && !config.google.ics_url.is_empty() {
            Some(Arc::new(IcsClient::new(
                config.google.ics_url.clone(),
                CalendarSource::Google,
            )))
        } else {
            None
        };

    let ms_ics = if !config.microsoft.ics_url.is_empty() {
        Some(Arc::new(IcsClient::new(
            config.microsoft.ics_url.clone(),
            CalendarSource::Microsoft,
        )))
    } else {
        None
    };

    (google_client, google_vdir, google_ics, ms_ics)
}
