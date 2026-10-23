use std::collections::HashSet;

use anyhow::{Context, Result};
use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, TimeZone};
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
            .error_for_status()
            .context("ICS URL returned an error (URL may have expired or changed)")?
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

    // Modified instances of a recurring meeting are separate VEVENTs sharing the
    // master's UID, with a RECURRENCE-ID naming the occurrence they replace.
    let overridden: HashSet<(String, DateTime<Local>)> = calendar
        .components
        .iter()
        .filter_map(|c| match c {
            CalendarComponent::Event(ev) => Some(ev),
            _ => None,
        })
        .filter_map(|ev| {
            let rid = ev.get_recurrence_id().and_then(to_local)?;
            Some((ev.property_value("UID").unwrap_or("").to_string(), rid))
        })
        .collect();

    let mut events = Vec::new();
    for component in &calendar.components {
        let CalendarComponent::Event(ev) = component else { continue };

        let Some(start) = ev.get_start().and_then(to_local) else { continue };
        let end = ev
            .get_end()
            .and_then(to_local)
            .unwrap_or_else(|| start + chrono::Duration::hours(1));
        let duration = end - start;

        let starts = if ev.property_value("RRULE").is_some() && ev.get_recurrence_id().is_none() {
            let uid = ev.property_value("UID").unwrap_or("").to_string();
            expand_recurrence(ev, day_start - duration, day_end)
                .unwrap_or_else(|| vec![start])
                .into_iter()
                .filter(|s| !overridden.contains(&(uid.clone(), *s)))
                .collect()
        } else {
            vec![start]
        };

        for start in starts {
            let end = start + duration;
            if start > day_end || end < day_start {
                continue;
            }
            events.push(build_event(ev, start, end, &source));
        }
    }

    events.sort_by_key(|e| e.start);
    Ok(events)
}

fn build_event(
    ev: &icalendar::Event,
    start: DateTime<Local>,
    end: DateTime<Local>,
    source: &CalendarSource,
) -> CalendarEvent {
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

    CalendarEvent {
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
    }
}

/// Occurrence start times of a recurring event (RRULE minus EXDATE) within
/// `[from, to]`. Returns `None` if the rule can't be interpreted.
fn expand_recurrence(
    ev: &icalendar::Event,
    from: DateTime<Local>,
    to: DateTime<Local>,
) -> Option<Vec<DateTime<Local>>> {
    let dt_start = to_rrule_tz(ev.get_start()?)?;
    let tz = dt_start.timezone();
    let rule: rrule::RRule<rrule::Unvalidated> = ev.property_value("RRULE")?.parse().ok()?;
    let mut set = rule.build(dt_start).ok()?;

    let exdates = ev
        .properties()
        .get("EXDATE")
        .into_iter()
        .chain(ev.multi_properties().get("EXDATE").into_iter().flatten());
    for prop in exdates {
        let tzid = prop.params().get("TZID").map(|p| p.value().to_string());
        for value in prop.value().split(',') {
            let d = match (value.trim(), &tzid) {
                (v, _) if v.ends_with('Z') => {
                    let naive = NaiveDateTime::parse_from_str(v, "%Y%m%dT%H%M%SZ").ok()?;
                    DatePerhapsTime::DateTime(CalendarDateTime::Utc(naive.and_utc()))
                }
                (v, _) if v.len() == 8 => {
                    DatePerhapsTime::Date(NaiveDate::parse_from_str(v, "%Y%m%d").ok()?)
                }
                (v, tzid) => {
                    let date_time = NaiveDateTime::parse_from_str(v, "%Y%m%dT%H%M%S").ok()?;
                    match tzid {
                        Some(tzid) => DatePerhapsTime::DateTime(CalendarDateTime::WithTimezone {
                            date_time,
                            tzid: tzid.clone(),
                        }),
                        None => DatePerhapsTime::DateTime(CalendarDateTime::Floating(date_time)),
                    }
                }
            };
            set = set.exdate(to_rrule_tz(d)?.with_timezone(&tz));
        }
    }

    // `after`/`before` are exclusive, so widen by a second to keep boundary hits.
    let one_sec = chrono::Duration::seconds(1);
    let result = set
        .after((from - one_sec).with_timezone(&tz))
        .before((to + one_sec).with_timezone(&tz))
        .all(u16::MAX);
    Some(result.dates.iter().map(|d| d.with_timezone(&Local)).collect())
}

fn to_rrule_tz(d: DatePerhapsTime) -> Option<DateTime<rrule::Tz>> {
    match d {
        DatePerhapsTime::DateTime(CalendarDateTime::WithTimezone { date_time, tzid }) => {
            let tz = rrule::Tz::from(resolve_tz(&tzid));
            tz.from_local_datetime(&date_time).earliest()
        }
        DatePerhapsTime::DateTime(CalendarDateTime::Utc(utc)) => {
            Some(utc.with_timezone(&rrule::Tz::UTC))
        }
        DatePerhapsTime::DateTime(CalendarDateTime::Floating(naive)) => {
            rrule::Tz::LOCAL.from_local_datetime(&naive).earliest()
        }
        DatePerhapsTime::Date(date) => {
            rrule::Tz::LOCAL.from_local_datetime(&date.and_hms_opt(0, 0, 0)?).earliest()
        }
    }
}

fn resolve_tz(tzid: &str) -> chrono_tz::Tz {
    windows_to_iana(tzid).unwrap_or(tzid).parse().unwrap_or(chrono_tz::UTC)
}

pub fn to_local(d: DatePerhapsTime) -> Option<DateTime<Local>> {
    match d {
        DatePerhapsTime::DateTime(cal_dt) => Some(match cal_dt {
            CalendarDateTime::Utc(utc) => utc.with_timezone(&Local),
            CalendarDateTime::Floating(naive) => Local.from_local_datetime(&naive).earliest()?,
            CalendarDateTime::WithTimezone { date_time, tzid } => {
                resolve_tz(&tzid)
                    .from_local_datetime(&date_time)
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

fn windows_to_iana(windows_tz: &str) -> Option<&'static str> {
    // Mapping from Windows timezone IDs to IANA IDs (from CLDR windowsZones.xml)
    match windows_tz {
        "Dateline Standard Time"          => Some("Etc/GMT+12"),
        "UTC-11"                          => Some("Etc/GMT+11"),
        "Aleutian Standard Time"          => Some("America/Adak"),
        "Hawaiian Standard Time"          => Some("Pacific/Honolulu"),
        "Marquesas Standard Time"         => Some("Pacific/Marquesas"),
        "Alaskan Standard Time"           => Some("America/Anchorage"),
        "UTC-09"                          => Some("Etc/GMT+9"),
        "Pacific Standard Time (Mexico)"  => Some("America/Tijuana"),
        "UTC-08"                          => Some("Etc/GMT+8"),
        "Pacific Standard Time"           => Some("America/Los_Angeles"),
        "US Mountain Standard Time"       => Some("America/Phoenix"),
        "Mountain Standard Time (Mexico)" => Some("America/Chihuahua"),
        "Mountain Standard Time"          => Some("America/Denver"),
        "Yukon Standard Time"             => Some("America/Whitehorse"),
        "Central America Standard Time"   => Some("America/Guatemala"),
        "Central Standard Time"           => Some("America/Chicago"),
        "Easter Island Standard Time"     => Some("Pacific/Easter"),
        "Central Standard Time (Mexico)"  => Some("America/Mexico_City"),
        "Canada Central Standard Time"    => Some("America/Regina"),
        "SA Pacific Standard Time"        => Some("America/Bogota"),
        "Eastern Standard Time (Mexico)"  => Some("America/Cancun"),
        "Eastern Standard Time"           => Some("America/New_York"),
        "Haiti Standard Time"             => Some("America/Port-au-Prince"),
        "Cuba Standard Time"              => Some("America/Havana"),
        "US Eastern Standard Time"        => Some("America/Indiana/Indianapolis"),
        "Turks And Caicos Standard Time"  => Some("America/Grand_Turk"),
        "Paraguay Standard Time"          => Some("America/Asuncion"),
        "Atlantic Standard Time"          => Some("America/Halifax"),
        "Venezuela Standard Time"         => Some("America/Caracas"),
        "Central Brazilian Standard Time" => Some("America/Cuiaba"),
        "SA Western Standard Time"        => Some("America/La_Paz"),
        "Pacific SA Standard Time"        => Some("America/Santiago"),
        "Newfoundland Standard Time"      => Some("America/St_Johns"),
        "Tocantins Standard Time"         => Some("America/Araguaina"),
        "E. South America Standard Time"  => Some("America/Sao_Paulo"),
        "SA Eastern Standard Time"        => Some("America/Cayenne"),
        "Argentina Standard Time"         => Some("America/Buenos_Aires"),
        "Greenland Standard Time"         => Some("America/Godthab"),
        "Montevideo Standard Time"        => Some("America/Montevideo"),
        "Magallanes Standard Time"        => Some("America/Punta_Arenas"),
        "Saint Pierre Standard Time"      => Some("America/Miquelon"),
        "Bahia Standard Time"             => Some("America/Bahia"),
        "UTC-02"                          => Some("Etc/GMT+2"),
        "Azores Standard Time"            => Some("Atlantic/Azores"),
        "Cape Verde Standard Time"        => Some("Atlantic/Cape_Verde"),
        "UTC"                             => Some("Etc/GMT"),
        "GMT Standard Time"               => Some("Europe/London"),
        "Greenwich Standard Time"         => Some("Atlantic/Reykjavik"),
        "Sao Tome Standard Time"          => Some("Africa/Sao_Tome"),
        "Morocco Standard Time"           => Some("Africa/Casablanca"),
        "W. Europe Standard Time"         => Some("Europe/Berlin"),
        "Central Europe Standard Time"    => Some("Europe/Budapest"),
        "Romance Standard Time"           => Some("Europe/Paris"),
        "Central European Standard Time"  => Some("Europe/Warsaw"),
        "W. Central Africa Standard Time" => Some("Africa/Lagos"),
        "GTB Standard Time"               => Some("Europe/Bucharest"),
        "Middle East Standard Time"       => Some("Asia/Beirut"),
        "Egypt Standard Time"             => Some("Africa/Cairo"),
        "E. Europe Standard Time"         => Some("Asia/Nicosia"),
        "Syria Standard Time"             => Some("Asia/Damascus"),
        "West Bank Standard Time"         => Some("Asia/Hebron"),
        "South Africa Standard Time"      => Some("Africa/Johannesburg"),
        "FLE Standard Time"               => Some("Europe/Kiev"),
        "Israel Standard Time"            => Some("Asia/Jerusalem"),
        "Kaliningrad Standard Time"       => Some("Europe/Kaliningrad"),
        "Sudan Standard Time"             => Some("Africa/Khartoum"),
        "Libya Standard Time"             => Some("Africa/Tripoli"),
        "Namibia Standard Time"           => Some("Africa/Windhoek"),
        "Jordan Standard Time"            => Some("Asia/Amman"),
        "Arabic Standard Time"            => Some("Asia/Baghdad"),
        "Turkey Standard Time"            => Some("Europe/Istanbul"),
        "Arab Standard Time"              => Some("Asia/Riyadh"),
        "Belarus Standard Time"           => Some("Europe/Minsk"),
        "Russian Standard Time"           => Some("Europe/Moscow"),
        "E. Africa Standard Time"         => Some("Africa/Nairobi"),
        "Volga Standard Time"             => Some("Europe/Saratov"),
        "Iran Standard Time"              => Some("Asia/Tehran"),
        "Arabian Standard Time"           => Some("Asia/Dubai"),
        "Astrakhan Standard Time"         => Some("Europe/Astrakhan"),
        "Azerbaijan Standard Time"        => Some("Asia/Baku"),
        "Russia Time Zone 3"              => Some("Europe/Samara"),
        "Mauritius Standard Time"         => Some("Indian/Mauritius"),
        "Saratov Standard Time"           => Some("Europe/Saratov"),
        "Georgian Standard Time"          => Some("Asia/Tbilisi"),
        "Caucasus Standard Time"          => Some("Asia/Yerevan"),
        "Afghanistan Standard Time"       => Some("Asia/Kabul"),
        "West Asia Standard Time"         => Some("Asia/Tashkent"),
        "Ekaterinburg Standard Time"      => Some("Asia/Yekaterinburg"),
        "Pakistan Standard Time"          => Some("Asia/Karachi"),
        "Qyzylorda Standard Time"         => Some("Asia/Qyzylorda"),
        "India Standard Time"             => Some("Asia/Calcutta"),
        "Sri Lanka Standard Time"         => Some("Asia/Colombo"),
        "Nepal Standard Time"             => Some("Asia/Katmandu"),
        "Central Asia Standard Time"      => Some("Asia/Almaty"),
        "Bangladesh Standard Time"        => Some("Asia/Dhaka"),
        "Omsk Standard Time"              => Some("Asia/Omsk"),
        "Myanmar Standard Time"           => Some("Asia/Rangoon"),
        "SE Asia Standard Time"           => Some("Asia/Bangkok"),
        "Altai Standard Time"             => Some("Asia/Barnaul"),
        "W. Mongolia Standard Time"       => Some("Asia/Hovd"),
        "North Asia Standard Time"        => Some("Asia/Krasnoyarsk"),
        "N. Central Asia Standard Time"   => Some("Asia/Novosibirsk"),
        "Tomsk Standard Time"             => Some("Asia/Tomsk"),
        "China Standard Time"             => Some("Asia/Shanghai"),
        "North Asia East Standard Time"   => Some("Asia/Irkutsk"),
        "Singapore Standard Time"         => Some("Asia/Singapore"),
        "W. Australia Standard Time"      => Some("Australia/Perth"),
        "Taipei Standard Time"            => Some("Asia/Taipei"),
        "Ulaanbaatar Standard Time"       => Some("Asia/Ulaanbaatar"),
        "Aus Central W. Standard Time"    => Some("Australia/Eucla"),
        "Transbaikal Standard Time"       => Some("Asia/Chita"),
        "Tokyo Standard Time"             => Some("Asia/Tokyo"),
        "North Korea Standard Time"       => Some("Asia/Pyongyang"),
        "Korea Standard Time"             => Some("Asia/Seoul"),
        "Yakutsk Standard Time"           => Some("Asia/Yakutsk"),
        "Cen. Australia Standard Time"    => Some("Australia/Adelaide"),
        "AUS Central Standard Time"       => Some("Australia/Darwin"),
        "E. Australia Standard Time"      => Some("Australia/Brisbane"),
        "AUS Eastern Standard Time"       => Some("Australia/Sydney"),
        "West Pacific Standard Time"      => Some("Pacific/Port_Moresby"),
        "Tasmania Standard Time"          => Some("Australia/Hobart"),
        "Vladivostok Standard Time"       => Some("Asia/Vladivostok"),
        "Lord Howe Standard Time"         => Some("Australia/Lord_Howe"),
        "Bougainville Standard Time"      => Some("Pacific/Bougainville"),
        "Russia Time Zone 10"             => Some("Asia/Srednekolymsk"),
        "Magadan Standard Time"           => Some("Asia/Magadan"),
        "Norfolk Standard Time"           => Some("Pacific/Norfolk"),
        "Sakhalin Standard Time"          => Some("Asia/Sakhalin"),
        "Central Pacific Standard Time"   => Some("Pacific/Guadalcanal"),
        "Russia Time Zone 11"             => Some("Asia/Kamchatka"),
        "New Zealand Standard Time"       => Some("Pacific/Auckland"),
        "UTC+12"                          => Some("Etc/GMT-12"),
        "Fiji Standard Time"              => Some("Pacific/Fiji"),
        "Chatham Islands Standard Time"   => Some("Pacific/Chatham"),
        "UTC+13"                          => Some("Etc/GMT-13"),
        "Tonga Standard Time"             => Some("Pacific/Tongatapu"),
        "Samoa Standard Time"             => Some("Pacific/Apia"),
        "Line Islands Standard Time"      => Some("Pacific/Kiritimati"),
        _                                 => None,
    }
}

fn partstat_to_rsvp(s: &str) -> ResponseStatus {
    match s.to_uppercase().as_str() {
        "ACCEPTED" => ResponseStatus::Accepted,
        "DECLINED" => ResponseStatus::Declined,
        "TENTATIVE" => ResponseStatus::Tentative,
        _ => ResponseStatus::NeedsAction,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RECURRING: &str = "BEGIN:VCALENDAR\r
VERSION:2.0\r
BEGIN:VEVENT\r
UID:weekly\r
SUMMARY:Weekly sync\r
DTSTART;TZID=W. Europe Standard Time:20260105T100000\r
DTEND;TZID=W. Europe Standard Time:20260105T103000\r
RRULE:FREQ=WEEKLY;INTERVAL=1;BYDAY=MO;WKST=MO\r
EXDATE;TZID=W. Europe Standard Time:20260119T100000\r
END:VEVENT\r
BEGIN:VEVENT\r
UID:weekly\r
SUMMARY:Weekly sync (moved)\r
RECURRENCE-ID;TZID=W. Europe Standard Time:20260126T100000\r
DTSTART;TZID=W. Europe Standard Time:20260127T140000\r
DTEND;TZID=W. Europe Standard Time:20260127T143000\r
END:VEVENT\r
END:VCALENDAR\r
";

    fn titles_on(y: i32, m: u32, d: u32) -> Vec<String> {
        let day = NaiveDate::from_ymd_opt(y, m, d).unwrap();
        parse_ics_text(RECURRING, day, CalendarSource::Microsoft)
            .unwrap()
            .into_iter()
            .map(|e| e.title)
            .collect()
    }

    #[test]
    fn expands_weekly_rrule() {
        assert_eq!(titles_on(2026, 1, 5), ["Weekly sync"]);
        assert_eq!(titles_on(2026, 1, 12), ["Weekly sync"]);
        assert_eq!(titles_on(2026, 3, 2), ["Weekly sync"]);
        assert!(titles_on(2026, 1, 13).is_empty());
    }

    #[test]
    fn honours_exdate() {
        assert!(titles_on(2026, 1, 19).is_empty());
    }

    #[test]
    fn override_replaces_occurrence() {
        assert!(titles_on(2026, 1, 26).is_empty());
        assert_eq!(titles_on(2026, 1, 27), ["Weekly sync (moved)"]);
    }
}
