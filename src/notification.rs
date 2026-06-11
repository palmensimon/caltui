use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};
use chrono::Local;

use crate::app::AppEvent;
use crate::calendar::CalendarEvent;
use crate::config::NotificationConfig;

/// Show a desktop notification.
///
/// On macOS, notify-rust's NSUserNotification backend silently fails for
/// unbundled CLI binaries on recent macOS versions, so we go through
/// osascript instead. Title/body are passed as argv so no escaping is needed.
pub fn send(summary: &str, body: &str) {
    #[cfg(target_os = "macos")]
    {
        let script = r#"on run argv
    display notification (item 2 of argv) with title (item 1 of argv)
end run"#;
        let _ = std::process::Command::new("osascript")
            .arg("-e")
            .arg(script)
            .arg(summary)
            .arg(body)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }

    #[cfg(not(target_os = "macos"))]
    {
        notify_rust::Notification::new()
            .summary(summary)
            .body(body)
            .timeout(notify_rust::Timeout::Milliseconds(8000))
            .show()
            .ok();
    }
}

pub fn spawn_notification_watcher(
    tx: mpsc::Sender<AppEvent>,
    events: Arc<RwLock<Vec<CalendarEvent>>>,
    notif_config: Arc<RwLock<NotificationConfig>>,
) {
    tokio::spawn(async move {
        let mut notified_before: HashSet<String> = HashSet::new();
        let mut notified_start: HashSet<String> = HashSet::new();
        loop {
            tokio::time::sleep(Duration::from_secs(30)).await;
            let _ = tx.send(AppEvent::Tick).await;

            let now = Local::now();
            let config = notif_config.read().await.clone();
            let guard = events.read().await;

            for event in guard.iter() {
                let secs = (event.start - now).num_seconds();

                if let Some(before) = config.notify_before_minutes {
                    let key = format!("before:{}", event.id);
                    if secs >= 0 && secs <= before as i64 * 60 && !notified_before.contains(&key) {
                        notified_before.insert(key);
                        send("caltui", &format!("In {} min: {}", before, event.title));
                    }
                }

                if config.notify_on_start {
                    let key = format!("start:{}", event.id);
                    if (0..60).contains(&secs) && !notified_start.contains(&key) {
                        notified_start.insert(key);
                        send("caltui", &format!("Starting now: {}", event.title));
                    }
                }
            }
        }
    });
}
