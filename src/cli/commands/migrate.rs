use std::path::PathBuf;

use crate::cli::stores::SqliteHistoryStore;

use super::{CommandResult, EXIT_ERROR, load_config_or_exit};

pub(super) async fn run(config: Option<PathBuf>) -> CommandResult {
    let config = load_config_or_exit(config.as_deref())?;
    SqliteHistoryStore::connect(&config.database.url)
        .await
        .map_err(|error| {
            eprintln!("Error: migration failed: {error}");
            EXIT_ERROR
        })?;
    println!("Database migrated successfully.");
    Ok(())
}
