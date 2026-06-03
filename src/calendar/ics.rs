use anyhow::{Context, Result};
use chrono::{DateTime, Local, NaiveDate, TimeZone};
use icalendar::{CalendarComponent, CalendarDateTime, Component, DatePerhapsTime, EventLike};

use crate::calendar::{extract_meeting_url, Attendee, CalendarEvent, CalendarSource, ResponseStatus};

pub struct IcsClient {
    http: reqwest::Client,
    url: String,
    source: CalendarSource,
}

impl IcsClient {
    pub fn new(url: String, source: CalendarSource) -> Self {
        Self { http: reqwest::Client::new(), url, source }
    }

    pub async fn list_events(&self, day: NaiveDate) -> Result<Vec<CalendarEvent>> {
        let text = self
            .http
            .get(&self.url)
            .send()
            .await
            .context("ICS fetch failed")?
            .text()
            .await
            .context("ICS read failed")?;

        parse_ics_text(&text, day, self.source.clone())
    }
}

pub fn parse_ics_text(text: &str, day: NaiveDate, source: CalendarSource) -> Result<Vec<CalendarEvent>> {
    let calendar = text
        .parse::<icalendar::Calendar>()
        .map_err(|e| anyhow::anyhow!("ICS parse failed: {e}"))?;

    let day_start = Local
        .from_local_datetime(&day.and_hms_opt(0, 0, 0).unwrap())
        .earliest()
        .unwrap();
    let day_end = Local
        .from_local_datetime(&day.and_hms_opt(23, 59, 59).unwrap())
        .earliest()
        .unwrap();

    let mut events = Vec::new();
    for component in &calendar.components {
        let CalendarComponent::Event(ev) = component else { continue };

        let Some(start) = ev.get_start().and_then(to_local) else { continue };
        let end = ev
            .get_end()
            .and_then(to_local)
            .unwrap_or_else(|| start + chrono::Duration::hours(1));

        if start > day_end || end < day_start {
            continue;
        }

        let id = ev.property_value("UID").unwrap_or("").to_string();
        let title = ev.get_summary().unwrap_or("(no title)").to_string();
        let description = ev.get_description().map(|s| s.to_string());
        let location = ev.get_location().map(|s| s.to_string());
        let meeting_url = extract_meeting_url(description.as_deref().unwrap_or(""), None);
        let event_url = ev.property_value("URL")
            .or_else(|| ev.property_value("X-MICROSOFT-URL"))
            .map(|s| s.to_string())
            .or_else(|| match source {
                CalendarSource::Microsoft => Some("https://outlook.office.com/calendar/".to_string()),
                _ => None,
            });
        let is_all_day = matches!(ev.get_start(), Some(DatePerhapsTime::Date(_)));

        let (attendees, response_status) = parse_attendees(ev);
        let organizer = parse_organizer(ev);
        let cancelled = ev.property_value("STATUS")
            .map(|s| s.to_uppercase() == "CANCELLED")
            .unwrap_or(false);

        events.push(CalendarEvent {
            id,
            title,
            start,
            end,
            description,
            location,
            meeting_url,
            event_url,
            source: source.clone(),
            response_status,
            organizer,
            attendees,
            is_all_day,
            cancelled,
        });
    }

    events.sort_by_key(|e| e.start);
    Ok(events)
}

pub fn to_local(d: DatePerhapsTime) -> Option<DateTime<Local>> {
    match d {
        DatePerhapsTime::DateTime(cal_dt) => Some(match cal_dt {
            CalendarDateTime::Utc(utc) => utc.with_timezone(&Local),
            CalendarDateTime::Floating(naive) => Local.from_local_datetime(&naive).earliest()?,
            CalendarDateTime::WithTimezone { date_time, tzid } => {
                let tz: chrono_tz::Tz = tzid.parse().unwrap_or(chrono_tz::UTC);
                tz.from_local_datetime(&date_time)
                    .earliest()?
                    .with_timezone(&Local)
            }
        }),
        DatePerhapsTime::Date(date) => {
            Local.from_local_datetime(&date.and_hms_opt(0, 0, 0)?).earliest()
        }
    }
}

fn parse_attendees(ev: &icalendar::Event) -> (Vec<Attendee>, ResponseStatus) {
    let mut attendees = Vec::new();
    let mut self_status = ResponseStatus::NeedsAction;

    if let Some(props) = ev.multi_properties().get("ATTENDEE") {
        for prop in props {
            let raw = prop.value();
            let email = raw
                .trim_start_matches("mailto:")
                .trim_start_matches("MAILTO:")
                .to_string();

            let params = prop.params();
            let partstat = params
                .get("PARTSTAT")
                .map(|p| p.value())
                .unwrap_or("NEEDS-ACTION");

            let response = partstat_to_rsvp(partstat);
            let cn = params.get("CN").map(|p| p.value().to_string());
            let is_rsvp = params
                .get("RSVP")
                .map(|p| p.value().eq_ignore_ascii_case("TRUE"))
                .unwrap_or(false);

            if is_rsvp {
                self_status = response.clone();
            }

            attendees.push(Attendee { email, name: cn, response, is_self: is_rsvp });
        }
    }

    if matches!(self_status, ResponseStatus::NeedsAction) {
        if let Some(status) = ev.property_value("STATUS") {
            self_status = match status.to_uppercase().as_str() {
                "CONFIRMED" => ResponseStatus::Accepted,
                "TENTATIVE" => ResponseStatus::Tentative,
                _ => ResponseStatus::NeedsAction,
            };
        }
    }

    (attendees, self_status)
}

fn parse_organizer(ev: &icalendar::Event) -> Option<String> {
    let prop = ev.properties().get("ORGANIZER")?;
    let cn = prop.params().get("CN").map(|p| p.value().to_string());
    if cn.is_some() {
        return cn;
    }
    let email = prop.value().trim_start_matches("mailto:").to_string();
    Some(email)
}

fn partstat_to_rsvp(s: &str) -> ResponseStatus {
    match s.to_uppercase().as_str() {
        "ACCEPTED" => ResponseStatus::Accepted,
        "DECLINED" => ResponseStatus::Declined,
        "TENTATIVE" => ResponseStatus::Tentative,
        _ => ResponseStatus::NeedsAction,
    }
}
