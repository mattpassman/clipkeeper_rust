use clap::Parser;
use clipkeeper::cli::{Cli, Commands};
use clipkeeper::errors::Result;

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Start { monitor, service } => clipkeeper::cli::handle_start(monitor, service),
        Commands::Stop => clipkeeper::cli::handle_stop(),
        Commands::Status => clipkeeper::cli::handle_status(),
        Commands::List { limit, content_type, search, since, format, no_interactive } => {
            clipkeeper::cli::handle_list(limit, content_type, search, since, format, no_interactive)
        }
        Commands::Search { query, limit, content_type, since, no_interactive } => {
            clipkeeper::cli::handle_search(&query, limit, content_type, since, no_interactive)
        }
        Commands::Copy { id } => clipkeeper::cli::handle_copy(&id),
        Commands::Clear { confirm } => clipkeeper::cli::handle_clear(confirm),
        Commands::Metrics { history, limit, clear } => clipkeeper::cli::handle_metrics(history, limit, clear),
        Commands::Config { action } => clipkeeper::cli::handle_config(action),
    }
}
