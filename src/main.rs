mod cli;
mod config;
mod clipboard_monitor;
mod clipboard_service;
mod history_store;
mod privacy_filter;
mod content_classifier;
mod service;
mod app;
mod errors;
mod logger;
mod resource_monitor;
mod retention_service;
mod search_service;

use clap::Parser;
use cli::{Cli, Commands};
use errors::Result;

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Start { monitor, service } => cli::handle_start(monitor, service),
        Commands::Stop => cli::handle_stop(),
        Commands::Status => cli::handle_status(),
        Commands::List { limit, content_type, search, since, format, no_interactive } => {
            cli::handle_list(limit, content_type, search, since, format, no_interactive)
        }
        Commands::Search { query, limit, content_type, since, no_interactive } => {
            cli::handle_search(&query, limit, content_type, since, no_interactive)
        }
        Commands::Copy { id } => cli::handle_copy(&id),
        Commands::Clear { confirm } => cli::handle_clear(confirm),
        Commands::Metrics { history, limit, clear } => cli::handle_metrics(history, limit, clear),
        Commands::Config { action } => cli::handle_config(action),
    }
}
