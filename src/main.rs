mod app;
mod calendar;
mod cli;
mod config;
mod notification;
mod tui;

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let config = config::load_config();

    match args.as_slice() {
        [] => tui::run_tui(config).await,
        ["next"] => cli::run_next(config, false).await,
        ["next", "--json"] | ["--json", "next"] => cli::run_next(config, true).await,
        ["-h" | "--help" | "help"] => {
            println!("{}", cli::USAGE);
            Ok(())
        }
        _ => {
            eprintln!("{}", cli::USAGE);
            std::process::exit(2);
        }
    }
}
