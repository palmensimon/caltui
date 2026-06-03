use std::path::{Path, PathBuf};
use anyhow::Result;
use chrono::NaiveDate;

use crate::calendar::{CalendarEvent, CalendarSource};
use super::ics::parse_ics_text;

pub struct VdirClient {
    path: PathBuf,
    source: CalendarSource,
}

impl VdirClient {
    pub fn new(path: String, source: CalendarSource) -> Self {
        Self { path: PathBuf::from(expand_tilde(&path)), source }
    }

    pub async fn list_events(&self, day: NaiveDate) -> Result<Vec<CalendarEvent>> {
        let path = self.path.clone();
        let source = self.source.clone();
        tokio::task::spawn_blocking(move || collect_events(&path, day, source))
            .await
            .map_err(|e| anyhow::anyhow!("vdir task failed: {e}"))?
    }
}

fn collect_events(path: &Path, day: NaiveDate, source: CalendarSource) -> Result<Vec<CalendarEvent>> {
    if !path.exists() {
        anyhow::bail!(
            "vdir path not found: {} — run `vdirsyncer sync` first",
            path.display()
        );
    }

    let mut events = Vec::new();
    read_dir_recursive(path, day, &source, &mut events, 0);
    events.sort_by_key(|e| e.start);
    Ok(events)
}

fn read_dir_recursive(
    path: &Path,
    day: NaiveDate,
    source: &CalendarSource,
    out: &mut Vec<CalendarEvent>,
    depth: u32,
) {
    let Ok(entries) = std::fs::read_dir(path) else { return };

    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() && depth == 0 {
            // Descend one level so users can point at the top-level vdir directory
            // and get all calendars, or at a specific calendar sub-directory.
            read_dir_recursive(&p, day, source, out, depth + 1);
        } else if p.extension().and_then(|e| e.to_str()) == Some("ics") {
            if let Ok(text) = std::fs::read_to_string(&p) {
                if let Ok(evs) = parse_ics_text(&text, day, source.clone()) {
                    out.extend(evs);
                }
            }
        }
    }
}

fn expand_tilde(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return format!("{}/{}", home.display(), rest);
        }
    }
    path.to_string()
}
