use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};
use chrono::Local;

use crate::app::AppEvent;
use crate::calendar::CalendarEvent;

pub fn spawn_notification_watcher(
    tx: mpsc::Sender<AppEvent>,
    events: Arc<RwLock<Vec<CalendarEvent>>>,
) {
    tokio::spawn(async move {
        let mut notified: std::collections::HashSet<String> = std::collections::HashSet::new();
        loop {
            tokio::time::sleep(Duration::from_secs(30)).await;
            let _ = tx.send(AppEvent::Tick).await;

            let now = Local::now();
            let guard = events.read().await;
            for event in guard.iter() {
                let mins = (event.start - now).num_minutes();
                if (0..=1).contains(&mins) && !notified.contains(&event.id) {
                    notified.insert(event.id.clone());
                    let title = event.title.clone();
                    notify_rust::Notification::new()
                        .summary("caltui")
                        .body(&format!("Starting soon: {}", title))
                        .timeout(notify_rust::Timeout::Milliseconds(8000))
                        .show()
                        .ok();
                }
            }
        }
    });
}
