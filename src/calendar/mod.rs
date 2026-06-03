pub mod google;
pub mod ics;
pub mod vdir;

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CalendarSource {
    Google,
    Microsoft,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ResponseStatus {
    Accepted,
    Declined,
    Tentative,
    NeedsAction,
}

impl ResponseStatus {
    pub fn display(&self) -> &str {
        match self {
            Self::Accepted => "accepted",
            Self::Declined => "declined",
            Self::Tentative => "tentative",
            Self::NeedsAction => "not responded",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attendee {
    pub email: String,
    pub name: Option<String>,
    pub response: ResponseStatus,
    pub is_self: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarEvent {
    pub id: String,
    pub title: String,
    pub start: DateTime<Local>,
    pub end: DateTime<Local>,
    pub description: Option<String>,
    pub location: Option<String>,
    pub meeting_url: Option<String>,
    pub event_url: Option<String>,
    pub source: CalendarSource,
    pub response_status: ResponseStatus,
    pub organizer: Option<String>,
    pub attendees: Vec<Attendee>,
    pub is_all_day: bool,
    pub cancelled: bool,
}

impl CalendarEvent {
    #[allow(dead_code)]
    pub fn is_past(&self) -> bool {
        self.start < Local::now()
    }

    pub fn has_meeting_link(&self) -> bool {
        self.meeting_url.is_some()
    }

    pub fn duration_minutes(&self) -> i64 {
        (self.end - self.start).num_minutes().max(1)
    }
}

pub fn extract_meeting_url(text: &str, hangout_link: Option<&str>) -> Option<String> {
    if let Some(link) = hangout_link {
        if !link.is_empty() {
            return Some(link.to_string());
        }
    }
    for word in text.split_whitespace() {
        let w = word.trim_matches(|c: char| {
            !c.is_alphanumeric()
                && c != ':'
                && c != '/'
                && c != '.'
                && c != '-'
                && c != '_'
                && c != '?'
                && c != '='
                && c != '&'
        });
        if w.contains("meet.google.com")
            || w.contains("teams.microsoft.com/l/meetup-join")
            || w.contains("zoom.us/j")
            || w.contains("zoom.us/s")
        {
            return Some(w.to_string());
        }
    }
    None
}
