# caltui

A terminal calendar that combines Google Calendar and Microsoft/Outlook in one timeline view.

## Features

- Unified day timeline for Google and Microsoft/Outlook events
- Join Teams and Google Meet meetings directly from the terminal
- Desktop notifications before meetings start

## Install

### Prerequisites

- [Rust](https://rustup.rs) (stable, 1.75+)
- A notification daemon (Linux: dunst, mako, or any libnotify-compatible daemon)
- On macOS, notifications are delivered via `osascript` — the first one triggers a permission prompt to allow notifications from Script Editor; accept it

```sh
cargo install --git https://github.com/palmensimon/caltui.git
```

The `caltui` binary will be placed in `~/.cargo/bin/`. Make sure that is on your `$PATH`.

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
