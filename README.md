# caltui

A terminal calendar that combines Google Calendar and Microsoft/Outlook in one timeline view.

## Features

- Unified day timeline for Google and Microsoft/Outlook events
- Join Teams and Google Meet meetings directly from the terminal
- Accept, decline or tentatively accept Google invitations (Outlook invitations open in the browser)
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

## Command line

`caltui next` prints the next upcoming meeting, for status bars and scripts:

```sh
$ caltui next
14:30 (in 25m) Design review
$ caltui next --json
{"title":"Design review","start":"2026-10-21T14:30:00+02:00","meeting_url":"https://meet.google.com/...","minutes_until":25,...}
```

It looks up to 7 days ahead and skips all-day, cancelled and declined events. With `--json` it prints the full event plus `minutes_until`, or `null` when nothing is coming up. If a calendar fails to load and no meeting was found, it prints the error to stderr and exits 1.

## Configuration

On first launch caltui will open the Settings screen. Press `?` for the setup guide.

Config is stored at `~/.config/caltui/config.toml`.

### Google Calendar

1. Create a project at [console.cloud.google.com](https://console.cloud.google.com)
2. Enable the **Google Calendar API**
3. Create OAuth credentials → Desktop app → copy Client ID and Client Secret
4. On the OAuth consent screen (Audience page), click **Publish app**. While the app is in "Testing" status, Google expires refresh tokens after 7 days and you'll have to sign in again every week. You don't need verification for personal use; you'll just click through an "unverified app" warning when you sign in.
5. Paste them into the Settings screen and press `g` to sign in

### Microsoft / Outlook

No app registration needed. Export your calendar as an ICS URL from Outlook:

- **Outlook Web**: Settings → View all settings → Calendar → Shared calendars → Publish a calendar → copy the ICS link
- Paste the ICS URL into field 3 in the Settings screen
