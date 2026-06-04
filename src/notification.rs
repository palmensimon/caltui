use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};
use chrono::Local;

use crate::app::AppEvent;
use crate::calendar::CalendarEvent;
use crate::config::NotificationConfig;

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
                        notify_rust::Notification::new()
                            .summary("caltui")
                            .body(&format!("In {} min: {}", before, event.title))
                            .timeout(notify_rust::Timeout::Milliseconds(8000))
                            .show()
                            .ok();
                    }
                }

                if config.notify_on_start {
                    let key = format!("start:{}", event.id);
                    if (0..60).contains(&secs) && !notified_start.contains(&key) {
                        notified_start.insert(key);
                        notify_rust::Notification::new()
                            .summary("caltui")
                            .body(&format!("Starting now: {}", event.title))
                            .timeout(notify_rust::Timeout::Milliseconds(8000))
                            .show()
                            .ok();
                    }
                }
            }
        }
    });
}
