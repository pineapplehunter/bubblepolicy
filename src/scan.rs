use color_eyre::Result;

pub fn run(paths: &[String]) -> Result<()> {
    if paths.is_empty() {
        color_eyre::bail!("Error: at least one path required");
    }

    println!("Scanning paths: {:?}", paths);
    println!("This would open a TUI file tree (requires ratatui integration)");

    Ok(())
}
