use anyhow::Result;
use chrono::{DateTime, Local, NaiveDate, TimeZone};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::calendar::{extract_meeting_url, Attendee, CalendarEvent, CalendarSource, ResponseStatus};
use crate::config::Config;

pub struct GoogleClient {
    config: Arc<Config>,
    http: reqwest::Client,
    cached_token: Arc<Mutex<Option<CachedToken>>>,
}

struct CachedToken {
    access_token: String,
    expires_at: DateTime<Local>,
}

impl GoogleClient {
    pub fn new(config: Arc<Config>) -> Self {
        Self {
            config,
            http: reqwest::Client::new(),
            cached_token: Arc::new(Mutex::new(None)),
        }
    }

    async fn access_token(&self) -> Result<String> {
        {
            let guard = self.cached_token.lock().await;
            if let Some(t) = &*guard {
                if t.expires_at > Local::now() + chrono::Duration::seconds(60) {
                    return Ok(t.access_token.clone());
                }
            }
        }

        let resp: serde_json::Value = self
            .http
            .post("https://oauth2.googleapis.com/token")
            .form(&[
                ("client_id", self.config.google.client_id.as_str()),
                ("client_secret", self.config.google.client_secret.as_str()),
                ("refresh_token", self.config.google.refresh_token.as_str()),
                ("grant_type", "refresh_token"),
            ])
            .send()
            .await?
            .json()
            .await?;

        if let Some(err) = resp["error"].as_str() {
            anyhow::bail!("token refresh failed: {}", err);
        }

        let access_token = resp["access_token"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("no access_token in refresh response"))?
            .to_string();
        let expires_in = resp["expires_in"].as_i64().unwrap_or(3600);
        let expires_at = Local::now() + chrono::Duration::seconds(expires_in);

        *self.cached_token.lock().await = Some(CachedToken { access_token: access_token.clone(), expires_at });
        Ok(access_token)
    }

    pub async fn list_events(&self, day: NaiveDate) -> Result<Vec<CalendarEvent>> {
        let token = self.access_token().await?;

        let day_start = Local
            .from_local_datetime(&day.and_hms_opt(0, 0, 0).unwrap())
            .earliest()
            .unwrap();
        let day_end = Local
            .from_local_datetime(&day.and_hms_opt(23, 59, 59).unwrap())
            .earliest()
            .unwrap();

        let resp: serde_json::Value = self
            .http
            .get("https://www.googleapis.com/calendar/v3/calendars/primary/events")
            .bearer_auth(&token)
            .query(&[
                ("timeMin", day_start.to_rfc3339()),
                ("timeMax", day_end.to_rfc3339()),
                ("singleEvents", "true".to_string()),
                ("orderBy", "startTime".to_string()),
            ])
            .send()
            .await?
            .json()
            .await?;

        if resp["error"].is_object() {
            anyhow::bail!("Calendar API error: {}", resp["error"]["message"].as_str().unwrap_or("unknown"));
        }

        let mut events = Vec::new();
        for item in resp["items"].as_array().into_iter().flatten() {
            let Some((start, end, is_all_day)) = parse_times(&item["start"], &item["end"]) else {
                continue;
            };
            let description = item["description"].as_str().map(|s| s.to_string());
            let hangout = item["hangoutLink"].as_str().map(|s| s.to_string());

            events.push(CalendarEvent {
                id: item["id"].as_str().unwrap_or("").to_string(),
                title: item["summary"].as_str().unwrap_or("(no title)").to_string(),
                start,
                end,
                meeting_url: extract_meeting_url(description.as_deref().unwrap_or(""), hangout.as_deref()),
                description,
                location: item["location"].as_str().map(|s| s.to_string()),
                event_url: item["htmlLink"].as_str().map(|s| s.to_string()),
                source: CalendarSource::Google,
                response_status: self_rsvp(&item["attendees"]),
                organizer: item["organizer"]["displayName"]
                    .as_str()
                    .or_else(|| item["organizer"]["email"].as_str())
                    .map(|s| s.to_string()),
                attendees: parse_attendees(&item["attendees"]),
                is_all_day,
                cancelled: item["status"].as_str() == Some("cancelled"),
            });
        }

        events.sort_by_key(|e| e.start);
        Ok(events)
    }

    /// Opens a browser OAuth flow and returns the refresh token.
    /// The caller should save it to config and rebuild clients.
    pub async fn start_oauth_flow(&self) -> Result<String> {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let port = listener.local_addr()?.port();
        let redirect_uri = format!("http://localhost:{port}");

        let auth_url = build_auth_url(&self.config.google.client_id, &redirect_uri);
        open::that_detached(&auth_url).ok();

        // Wait up to 2 minutes for the browser callback.
        let (mut stream, _) = tokio::time::timeout(
            std::time::Duration::from_secs(120),
            listener.accept(),
        )
        .await
        .map_err(|_| anyhow::anyhow!("auth timed out — sign-in not completed within 2 minutes"))??;

        // Read the first line of the HTTP request to get the redirect URL.
        let mut reader = BufReader::new(&mut stream);
        let mut request_line = String::new();
        reader.read_line(&mut request_line).await?;

        let code = extract_code(&request_line)
            .ok_or_else(|| anyhow::anyhow!("no authorization code in browser callback"))?;

        // Send a friendly response to the browser tab.
        let html = "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n\
            <html><body style='font-family:sans-serif;text-align:center;padding:3em'>\
            <h2>Authenticated!</h2><p>You can close this tab and return to caltui.</p>\
            </body></html>";
        stream.write_all(html.as_bytes()).await.ok();
        drop(stream);

        // Exchange the code for tokens.
        let resp: serde_json::Value = self
            .http
            .post("https://oauth2.googleapis.com/token")
            .form(&[
                ("client_id", self.config.google.client_id.as_str()),
                ("client_secret", self.config.google.client_secret.as_str()),
                ("code", code.as_str()),
                ("redirect_uri", redirect_uri.as_str()),
                ("grant_type", "authorization_code"),
            ])
            .send()
            .await?
            .json()
            .await?;

        if let Some(err) = resp["error"].as_str() {
            anyhow::bail!("token exchange failed: {} — {}", err, resp["error_description"].as_str().unwrap_or(""));
        }

        resp["refresh_token"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("no refresh_token in response — did you include access_type=offline?"))
            .map(|s| s.to_string())
    }
}

fn build_auth_url(client_id: &str, redirect_uri: &str) -> String {
    let params = [
        ("client_id", client_id),
        ("redirect_uri", redirect_uri),
        ("response_type", "code"),
        ("scope", "https://www.googleapis.com/auth/calendar.readonly"),
        ("access_type", "offline"),
        ("prompt", "consent"),
    ];
    let query = params
        .iter()
        .map(|(k, v)| format!("{}={}", k, percent_encode(v)))
        .collect::<Vec<_>>()
        .join("&");
    format!("https://accounts.google.com/o/oauth2/v2/auth?{query}")
}

fn extract_code(request_line: &str) -> Option<String> {
    // "GET /?code=XXXX&scope=... HTTP/1.1"
    let path = request_line.split_whitespace().nth(1)?;
    let query = path.split('?').nth(1)?;
    query.split('&').find_map(|pair| {
        let mut kv = pair.splitn(2, '=');
        if kv.next() == Some("code") {
            kv.next().map(|v| percent_decode(v))
        } else {
            None
        }
    })
}

fn percent_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn percent_decode(s: &str) -> String {
    let mut out = String::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
                out.push((h << 4 | l) as char);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn parse_times(
    start: &serde_json::Value,
    end: &serde_json::Value,
) -> Option<(DateTime<Local>, DateTime<Local>, bool)> {
    if let (Some(s), Some(e)) = (start["dateTime"].as_str(), end["dateTime"].as_str()) {
        let s = DateTime::parse_from_rfc3339(s).ok()?.with_timezone(&Local);
        let e = DateTime::parse_from_rfc3339(e).ok()?.with_timezone(&Local);
        Some((s, e, false))
    } else if let (Some(s), Some(e)) = (start["date"].as_str(), end["date"].as_str()) {
        let s_date = NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()?;
        let e_date = NaiveDate::parse_from_str(e, "%Y-%m-%d").ok()?;
        let s = Local.from_local_datetime(&s_date.and_hms_opt(0, 0, 0)?).earliest()?;
        let e = Local.from_local_datetime(&e_date.and_hms_opt(0, 0, 0)?).earliest()?;
        Some((s, e, true))
    } else {
        None
    }
}

fn self_rsvp(attendees: &serde_json::Value) -> ResponseStatus {
    if let Some(arr) = attendees.as_array() {
        for att in arr {
            if att["self"].as_bool().unwrap_or(false) {
                return rsvp_from_str(att["responseStatus"].as_str().unwrap_or("needsAction"));
            }
        }
        // Has attendees but user isn't listed — they're the organizer
        if !arr.is_empty() {
            return ResponseStatus::Accepted;
        }
    }
    ResponseStatus::Accepted
}

fn parse_attendees(attendees: &serde_json::Value) -> Vec<Attendee> {
    attendees
        .as_array()
        .into_iter()
        .flatten()
        .map(|att| Attendee {
            email: att["email"].as_str().unwrap_or("").to_string(),
            name: att["displayName"].as_str().map(|s| s.to_string()),
            response: rsvp_from_str(att["responseStatus"].as_str().unwrap_or("needsAction")),
            is_self: att["self"].as_bool().unwrap_or(false),
        })
        .collect()
}

fn rsvp_from_str(s: &str) -> ResponseStatus {
    match s {
        "accepted" => ResponseStatus::Accepted,
        "declined" => ResponseStatus::Declined,
        "tentative" => ResponseStatus::Tentative,
        _ => ResponseStatus::NeedsAction,
    }
}
