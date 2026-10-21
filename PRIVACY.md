# Privacy Policy

_Last updated: 2026-10-21_

caltui is an open-source terminal calendar that runs entirely on your own computer. It has no server, no accounts and no analytics.

## Data caltui accesses

- **Google Calendar.** With your permission, caltui requests the `https://www.googleapis.com/auth/calendar.events` scope. It reads events from your primary calendar: titles, times, descriptions, locations, organizers, attendees and meeting links. The only change it ever makes is setting your own response (accept, decline or maybe) on an invitation, and only when you press the key to do so. It never creates or deletes events and never edits other details.
- **Calendar feeds you configure.** If you add an ICS URL (for example an Outlook published calendar), caltui downloads that feed. If you configure a local calendar folder, caltui reads the `.ics` files in it.

## How the data is used

Calendar data is used only to show your events in the terminal and to send desktop notifications on your machine. Events are kept in memory while caltui is running and are not written to disk.

## What is stored

caltui stores its settings in `~/.config/caltui/config.toml` on your computer. That file contains your Google OAuth client ID and secret, your Google refresh token, and any calendar URLs or folders you configured. Nothing is stored anywhere else.

## Sharing

caltui does not send your data to the developer or to any third party. Its only network connections are to Google's OAuth and Calendar API endpoints (`accounts.google.com`, `oauth2.googleapis.com`, `www.googleapis.com`) and to the calendar URLs you configure yourself.

caltui's use of information received from Google APIs adheres to the [Google API Services User Data Policy](https://developers.google.com/terms/api-services-user-data-policy), including the Limited Use requirements.

## Removing your data

Delete `~/.config/caltui/config.toml` to remove everything caltui has stored. To revoke caltui's access to your Google account, go to [myaccount.google.com/connections](https://myaccount.google.com/connections).

## Contact

Questions can be filed as issues at [github.com/palmensimon/caltui](https://github.com/palmensimon/caltui/issues).
