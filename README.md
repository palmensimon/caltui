# caltui

A terminal calendar that combines Google Calendar and Microsoft/Outlook in one timeline view.

![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)

## Features

- Unified day timeline for Google and Microsoft/Outlook events
- Join Teams and Google Meet meetings directly from the terminal
- Desktop notifications before meetings start
- Colour-coded by source (green = Google, cyan = Microsoft)
- Dashed borders for unanswered invitations, strikethrough for cancelled events

## Install

### Prerequisites

- [Rust](https://rustup.rs) (stable, 1.75+)
- A notification daemon (Linux: dunst, mako, or any libnotify-compatible daemon)

### From source

```sh
git clone https://github.com/simonpalm/caltui
cd caltui
cargo install --path .
```

The `caltui` binary will be placed in `~/.cargo/bin/`. Make sure that is on your `$PATH`.

### Run without installing

```sh
cargo run --release
```

## Configuration

On first launch caltui will open the Settings screen. Press `?` for the setup guide.

Config is stored at `~/.config/caltui/config.toml`.

### Google Calendar

1. Create a project at [console.cloud.google.com](https://console.cloud.google.com)
2. Enable the **Google Calendar API**
3. Create OAuth credentials → Desktop app → copy Client ID and Client Secret
4. Paste them into the Settings screen and press `g` to sign in

### Microsoft / Outlook

No app registration needed. Export your calendar as an ICS URL from Outlook:

- **Outlook Web**: Settings → View all settings → Calendar → Shared calendars → Publish a calendar → copy the ICS link
- Paste the ICS URL into field 3 in the Settings screen

## Keybinds

### Timeline

| Key | Action |
|-----|--------|
| `j` / `k` | Select next / previous event |
| `Enter` | Open event detail |
| `o` | Join meeting (Teams app or browser) |
| `b` | Open event in browser (RSVP, remove, etc.) |
| `[` / `]` | Previous / next day |
| `t` | Jump to today and scroll to current time |
| `r` | Refresh events |
| `s` | Settings |
| `?` | Toggle help |
| `q` | Quit |

### Event detail

| Key | Action |
|-----|--------|
| `o` | Join meeting |
| `b` | Open event in browser |
| `↑` / `↓` | Scroll |
| `Esc` | Back to timeline |

## License

MIT — see [LICENSE](LICENSE).
