use color_eyre::{
    eyre::{bail, WrapErr},
    Result,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::process::{Command, Stdio};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct FileAccess {
    pub path: String,
    pub read: bool,
    pub write: bool,
    pub execute: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TraceOutput {
    pub files: Vec<FileAccess>,
}

pub fn run(cmd: &[String], output: Option<&str>) -> Result<()> {
    if cmd.is_empty() {
        bail!("No command provided");
    }

    let binary = &cmd[0];
    let args = &cmd[1..];

    eprintln!("Tracing: {} {:?}", binary, args);

    // Run strace and capture output
    let strace_output = Command::new("strace")
        .args(["-f", "-e", "trace=openat,open,read,write"])
        .arg(binary)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .context("Failed to run strace. Is strace installed?")?;

    let strace_output_str = String::from_utf8_lossy(&strace_output.stderr);

    // Parse strace output
    let file_accesses = parse_strace_output(&strace_output_str)?;

    // Deduplicate and output JSON
    let trace_output = TraceOutput {
        files: file_accesses,
    };

    let json =
        serde_json::to_string_pretty(&trace_output).context("Failed to serialize to JSON")?;

    if let Some(output_path) = output {
        fs::write(output_path, &json).context("Failed to write output file")?;
        eprintln!("Trace written to: {}", output_path);
    } else {
        println!("{}", json);
    }

    Ok(())
}

fn parse_strace_output(output: &str) -> Result<Vec<FileAccess>> {
    let mut file_map: BTreeMap<String, FileAccess> = BTreeMap::new();

    for line in output.lines() {
        // Parse openat syscalls: openat(AT_FDCWD, "/path/to/file", ...)
        if let Some(path) = extract_path_from_openat(line) {
            update_file_access(&mut file_map, path, line);
        }

        // Parse open syscalls: open("/path/to/file", ...)
        if line.contains("open(") && !line.contains("openat") {
            if let Some(path) = extract_path_from_open(line) {
                update_file_access(&mut file_map, path, line);
            }
        }
    }

    // Sort and return
    let mut files: Vec<FileAccess> = file_map.into_values().collect();
    files.sort();

    Ok(files)
}

fn update_file_access(
    file_map: &mut BTreeMap<String, FileAccess>,
    path: String,
    strace_line: &str,
) {
    let entry = file_map.entry(path.clone()).or_insert(FileAccess {
        path,
        read: false,
        write: false,
        execute: false,
    });

    // Detect read/write from flags
    // Common patterns: O_RDONLY, O_WRONLY, O_RDWR, O_CREAT, O_APPEND, etc.
    if strace_line.contains("O_RDONLY") {
        entry.read = true;
    }
    if strace_line.contains("O_WRONLY") {
        entry.write = true;
    }
    if strace_line.contains("O_RDWR") {
        entry.read = true;
        entry.write = true;
    }
    if strace_line.contains("O_CREAT") || strace_line.contains("O_APPEND") {
        entry.write = true;
    }

    // If no explicit flags, assume read-only (many files are opened for reading)
    if !entry.read && !entry.write {
        entry.read = true;
    }
}

fn extract_path_from_openat(line: &str) -> Option<String> {
    // Format: openat(AT_FDCWD, "/path", ...) or similar
    // Must start with openat
    if !line.contains("openat(") {
        return None;
    }

    // Find opening quote after openat(
    if let Some(start) = line.find("openat(") {
        let after_openat = &line[start + 7..];
        // Skip past AT_FDCWD or similar first argument
        if let Some(first_comma) = after_openat.find(',') {
            let after_first_comma = &after_openat[first_comma + 1..];
            // Now extract the quoted string
            extract_quoted_string(after_first_comma)
        } else {
            None
        }
    } else {
        None
    }
}

fn extract_path_from_open(line: &str) -> Option<String> {
    // Format: open("/path", ...)
    // Skip if it's openat
    if line.contains("openat") {
        return None;
    }

    // Find opening quote after open(
    if let Some(start) = line.find("open(") {
        let after_open = &line[start + 5..];
        extract_quoted_string(after_open)
    } else {
        None
    }
}

fn extract_quoted_string(s: &str) -> Option<String> {
    let s = s.trim();
    // Look for opening quote
    if let Some(quote_start) = s.find('"') {
        let after_quote = &s[quote_start + 1..];
        // Look for closing quote
        if let Some(quote_end) = after_quote.find('"') {
            let path = &after_quote[..quote_end];
            // Only return valid paths
            if !path.is_empty() && path.starts_with('/') {
                return Some(path.to_string());
            }
        }
    }
    None
}
