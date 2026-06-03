# caltui — project reference for AI agents

## What this is
A terminal UI calendar written in Rust. Combines Google Calendar (OAuth) and Microsoft/Outlook (ICS) into a single timeline view.

## Tech stack
- **ratatui 0.29** — TUI framework
- **tokio full** — async runtime
- **reqwest 0.12** (rustls) — HTTP
- **chrono** — date/time (always import `Timelike` when using `.hour()` / `.minute()`)
- **icalendar** — ICS parsing
- **notify-rust 4** — desktop notifications
- **tui-textarea 0.7** — editable text fields in settings
- **open 5** — open URLs in browser
- **dirs 5**, **toml 0.8**, **anyhow**, **serde_json**

## Module map

| Path | Responsibility |
|------|---------------|
| `src/main.rs` | Entry point, tokio runtime setup |
| `src/app.rs` | `App` state, `AppEvent` enum, day navigation, client orchestration |
| `src/config.rs` | `Config` struct, `~/.config/caltui/config.toml` read/write |
| `src/notification.rs` | Background watcher, fires notify-rust 5 min before events |
| `src/calendar/mod.rs` | `CalendarEvent`, `CalendarSource`, `ResponseStatus`, `extract_meeting_url()` |
| `src/calendar/google.rs` | `GoogleClient` — OAuth2 PKCE + Calendar API v3 |
| `src/calendar/ics.rs` | `IcsClient` — HTTP fetch + icalendar parse |
| `src/calendar/vdir.rs` | `VdirClient` — read local vdir directories |
| `src/tui/mod.rs` | Main event loop, terminal setup/teardown, `default_timeline_scroll()` |
| `src/tui/views/timeline.rs` | Timeline draw + `handle_key` |
| `src/tui/views/event_detail.rs` | Detail view draw + `handle_key`, Teams info extraction |
| `src/tui/views/settings.rs` | Settings fields (tui-textarea), Google auth flow trigger |
| `src/tui/views/help.rs` | Help overlay, context-aware by view |

## Key design decisions

### Auth / data sources
- **Google**: OAuth2 PKCE desktop flow. `client_id` + `client_secret` + `refresh_token` in config. `GoogleClient` handles token refresh automatically.
- **Microsoft / Outlook**: ICS URL only — no OAuth, no app registration. User pastes the Outlook "publish calendar" ICS link.
- **vdir fallback**: If a `google.vdir_path` is set and no OAuth is configured, `VdirClient` reads `.ics` files from a local directory (e.g. vdirsyncer sync target).

### Scroll state
- `timeline_scroll: i32` lives in `tui/mod.rs`'s event loop, measured in minutes from `start_hour`.
- `default_timeline_scroll()` returns minutes-to-current-time for today, 0 for other days.
- **Do not** reset scroll on `AppEvent::Tick` — it must stay stable during background refreshes.
- `app.scroll_to_now = true` signals mod.rs to snap scroll to current time (used by `go_today()`).
- Day navigation (`[` / `]`) resets scroll to 0 (top = start_hour, typically 08:00).

### Thin event boxes (height ≤ 2)
- height 1: `▌ Title` — colored `▌` as left-edge indicator
- height 2: `─────` top border line + `│ Title  10:00-10:30` content row
- height ≥ 3: full `Block::borders(Borders::ALL)`
- Always render `Clear` on the rect first to erase hour-grid lines underneath.

### Meeting links
- `extract_meeting_url()` in `calendar/mod.rs` scans description text for Google Meet, Teams, Zoom URLs.
- `join_meeting()` in `event_detail.rs` tries `msteams:` deep link first for Teams URLs, falls back to browser.
- `extract_teams_info()` in `event_detail.rs` parses "Meeting ID:" and "Passcode:" lines from description for display.

### Keybinds (timeline)
| Key | Action |
|-----|--------|
| `j` / `k` | Select next/prev event |
| `[` / `←` | Previous day (resets scroll to top) |
| `]` / `→` | Next day (resets scroll to top) |
| `t` | Jump to today AND scroll to current time |
| `Enter` | Open event detail |
| `r` | Refresh events |
| `s` | Settings |
| `?` | Toggle help overlay |
| `q` | Quit |

### Settings view extras
- `g` — trigger Google OAuth flow (opens browser)
- `n` — send test desktop notification
- `Ctrl+S` — save config

## Config file location
`~/.config/caltui/config.toml`

Fields: `google.client_id`, `google.client_secret`, `google.refresh_token`, `google.vdir_path`, `google.ics_url`, `microsoft.ics_url`, `display.start_hour` (default 8), `display.end_hour` (default 18).

## Build / run
```
cargo build
cargo run
```
No special environment variables required beyond having the config file populated.
