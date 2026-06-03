mod app;
mod calendar;
mod config;
mod notification;
mod tui;

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    let config = config::load_config();
    tui::run_tui(config).await
}
