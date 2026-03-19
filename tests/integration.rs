use std::fs;
use std::path::Path;
use std::process::Command;

fn get_bin_path() -> String {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    format!("{}/target/debug/bubblepolicy", manifest_dir)
}

#[test]
fn test_trace_and_create() {
    let bin_path = get_bin_path();
    let policy_path = "/tmp/bubblepolicy_integration_test.policy";
    let wrapper_path = "/tmp/bubblepolicy_wrapper_test.sh";

    let _ = fs::remove_file(policy_path);
    let _ = fs::remove_file(wrapper_path);

    let output = Command::new(&bin_path)
        .args(["trace", policy_path, "--", "echo", "hello"])
        .output()
        .expect("Failed to run trace");

    assert!(
        output.status.success(),
        "trace command failed: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(Path::new(policy_path).exists(), "Policy file not created");

    let policy_data = fs::read_to_string(policy_path).expect("Failed to read policy");
    for line in policy_data.lines() {
        if line.contains("/bin") || line.contains("/lib") {
            let parts: Vec<&str> = line.splitn(2, ' ').collect();
            if parts.len() == 2 {
                let new_line = format!("ReadOnly {}", parts[1]);
                let policy_data = policy_data.replace(line, &new_line);
                fs::write(policy_path, policy_data).unwrap();
                break;
            }
        }
    }

    let output = Command::new(&bin_path)
        .args(["create", policy_path, "echo"])
        .output()
        .expect("Failed to run create");

    assert!(
        output.status.success(),
        "create command failed: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("#!/bin/bash"),
        "Output should be a bash script"
    );
    assert!(stdout.contains("--ro-bind"), "Should have ro-bind mounts");

    let policy_data = fs::read_to_string(policy_path).expect("Failed to read policy");
    assert!(
        policy_data.contains("ReadOnly"),
        "Policy should have ReadOnly entries"
    );

    let _ = fs::remove_file(policy_path);
    let _ = fs::remove_file(wrapper_path);
}

#[test]
fn test_trace_produces_valid_output() {
    let bin_path = get_bin_path();
    let policy_path = "/tmp/bubblepolicy_test_valid.policy";

    let _ = fs::remove_file(policy_path);

    let output = Command::new(&bin_path)
        .args(["trace", policy_path, "--", "ls"])
        .output()
        .expect("Failed to run trace");

    assert!(output.status.success());
    assert!(Path::new(policy_path).exists());

    let data = fs::read_to_string(policy_path).expect("Failed to read policy");
    assert!(!data.is_empty());

    let mut has_valid_line = false;
    for line in data.lines() {
        let parts: Vec<&str> = line.splitn(2, ' ').collect();
        if parts.len() == 2
            && (parts[0] == "ReadOnly"
                || parts[0] == "ReadWrite"
                || parts[0] == "Tmpfs"
                || parts[0] == "Deny")
        {
            has_valid_line = true;
            break;
        }
    }
    assert!(has_valid_line, "Policy should have valid entries");

    let _ = fs::remove_file(policy_path);
}

#[test]
fn test_create_requires_existing_policy() {
    let bin_path = get_bin_path();

    let output = Command::new(&bin_path)
        .args(["create", "/nonexistent/path.policy", "echo"])
        .output()
        .expect("Failed to run create");

    assert!(!output.status.success());
}

#[test]
fn test_create_with_nested_path() {
    let bin_path = get_bin_path();
    let policy_path = "/tmp/bubblepolicy_test_nested.policy";

    let _ = fs::remove_file(policy_path);

    let policy = "ReadOnly /nix/store";
    fs::write(policy_path, policy).expect("Failed to write policy");

    let output = Command::new(&bin_path)
        .args(["create", policy_path, "/bin/sh"])
        .output()
        .expect("Failed to run create");

    assert!(output.status.success(), "create should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("--ro-bind /nix"),
        "Output should contain --ro-bind /nix, got: {}",
        stdout
    );

    let bind_lines: Vec<&str> = stdout.lines().filter(|l| l.contains("--ro-bind")).collect();

    for line in &bind_lines {
        assert!(
            !line.contains("--ro-bind / /"),
            "Should not have --ro-bind / /, got: {}",
            line
        );
    }

    let _ = fs::remove_file(policy_path);
}
