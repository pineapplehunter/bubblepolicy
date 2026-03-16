use color_eyre::{eyre::WrapErr, Result};

pub fn run(cmd: &[String]) -> Result<()> {
    let binary = &cmd[0];
    let args = &cmd[1..];

    println!("Tracing: {} {:?}", binary, args);
    println!("Output will be written to trace.json");

    use std::process::{Command, Stdio};

    let output = Command::new("strace")
        .args([
            "-f",
            "-e",
            "trace=file,open,openat,read,write,execve",
            "-o",
            "/dev/stdout",
        ])
        .arg(binary)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context("Failed to run strace")?;

    if !output.status.success() {
        color_eyre::bail!(
            "Command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    println!("{}", stderr);

    Ok(())
}
