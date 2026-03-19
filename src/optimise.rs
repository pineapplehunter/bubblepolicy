use color_eyre::{eyre::WrapErr, Result};
use log::info;
use std::fs;

use crate::common::{dedup_entries, entries_to_string, parse_entries};

pub fn run(file: &str) -> Result<()> {
    let data =
        fs::read_to_string(file).with_context(|| format!("Failed to read file: {}", file))?;

    let entries = parse_entries(&data);
    let optimised = dedup_entries(&entries);
    let output = entries_to_string(&optimised);

    fs::write(file, output).with_context(|| format!("Failed to write file: {}", file))?;

    info!(
        "Optimised: {} ({} -> {})",
        file,
        entries.len(),
        optimised.len()
    );
    Ok(())
}
