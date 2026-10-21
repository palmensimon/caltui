use std::sync::Arc;
use anyhow::Result;
use chrono::{Duration, Local};
use serde::Serialize;

use crate::app::{build_clients, fetch_events};
use crate::calendar::{CalendarEvent, ResponseStatus};
use crate::config::Config;

/// How many days ahead `caltui next` looks before giving up.
const LOOKAHEAD_DAYS: i64 = 7;

pub const USAGE: &str = "\
Usage:
  caltui               Open the calendar TUI
  caltui next          Print the next upcoming meeting
  caltui next --json   Print the next upcoming meeting as JSON (null if none)
  caltui --help        Show this help";

#[derive(Serialize)]
struct NextJson<'a> {
    #[serde(flatten)]
    event: &'a CalendarEvent,
    minutes_until: i64,
}

/// `caltui next [--json]`. Prints the first meeting that hasn't started yet,
/// skipping all-day, cancelled and declined events.
pub async fn run_next(config: Config, json: bool) -> Result<()> {
    let (google_client, google_vdir, google_ics, ms_ics) = build_clients(Arc::new(config));
    if google_client.is_none() && google_vdir.is_none() && google_ics.is_none() && ms_ics.is_none() {
        anyhow::bail!("no calendars configured — run caltui to set one up");
    }

    let now = Local::now();
    let today = now.date_naive();
    let mut next: Option<CalendarEvent> = None;
    let mut errors: Vec<String> = Vec::new();

    for offset in 0..LOOKAHEAD_DAYS {
        let day = today + Duration::days(offset);
        let (events, errs) = fetch_events(
            day,
            google_client.clone(),
            google_vdir.clone(),
            google_ics.clone(),
            ms_ics.clone(),
        )
        .await;
        errors.extend(errs);
        next = events
            .into_iter()
            .filter(|e| {
                e.start > now
                    && !e.is_all_day
                    && !e.cancelled
                    && e.response_status != ResponseStatus::Declined
            })
            .min_by_key(|e| e.start);
        if next.is_some() || !errors.is_empty() {
            break;
        }
    }

    // Report failures only when they may have hidden the answer: an event found
    // before the failing day is still the right one.
    if next.is_none() && !errors.is_empty() {
        errors.dedup();
        anyhow::bail!(errors.join("; "));
    }

    if json {
        let out = next.as_ref().map(|event| NextJson {
            event,
            minutes_until: (event.start - now).num_minutes(),
        });
        println!("{}", serde_json::to_string(&out)?);
    } else {
        match &next {
            Some(event) => println!("{}", format_next(event, now)),
            None => println!("No upcoming meetings"),
        }
    }
    Ok(())
}

fn format_next(event: &CalendarEvent, now: chrono::DateTime<Local>) -> String {
    let mins = (event.start - now).num_minutes();
    let when = if event.start.date_naive() == now.date_naive() {
        let rel = if mins < 60 {
            format!("{mins}m")
        } else {
            format!("{}h {}m", mins / 60, mins % 60)
        };
        format!("{} (in {rel})", event.start.format("%H:%M"))
    } else if event.start.date_naive() == now.date_naive() + Duration::days(1) {
        format!("Tomorrow {}", event.start.format("%H:%M"))
    } else {
        event.start.format("%a %H:%M").to_string()
    };
    format!("{when} {}", event.title)
}
