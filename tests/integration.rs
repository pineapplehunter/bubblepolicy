use std::fs;
use std::path::Path;
use std::process::Command;

fn get_bin_path() -> String {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    format!("{}/target/debug/myjail", manifest_dir)
}

fn setup_policy_with_ro(
    policy_path: &str,
    paths_to_allow: &[&str],
) -> Result<(), Box<dyn std::error::Error>> {
    let data = fs::read_to_string(policy_path)?;
    let mut trees: Vec<serde_json::Value> = serde_json::from_str(&data)?;

    for tree in &mut trees {
        if let Some(entries) = tree.get_mut("entries").and_then(|e| e.as_array_mut()) {
            for entry in entries {
                modify_entry(entry, paths_to_allow);
            }
        }
    }

    let json = serde_json::to_string_pretty(&trees)?;
    fs::write(policy_path, json)?;
    Ok(())
}

fn modify_entry(entry: &mut serde_json::Value, allowed: &[&str]) -> bool {
    let mut modified = false;
    if let Some(path) = entry.get("path").and_then(|p| p.as_str()) {
        if allowed.iter().any(|p| path.contains(p)) {
            entry["access"] = serde_json::Value::String("ReadOnly".to_string());
            modified = true;
        }
    }
    if let Some(children) = entry.get_mut("children").and_then(|c| c.as_array_mut()) {
        for child in children.iter_mut() {
            if modify_entry(child, allowed) {
                modified = true;
            }
        }
    }
    modified
}

#[test]
fn test_trace_and_create() {
    let bin_path = get_bin_path();
    let policy_path = "/tmp/myjail_integration_test.json";
    let wrapper_path = "/tmp/myjail_wrapper_test.sh";

    // Clean up
    let _ = fs::remove_file(policy_path);
    let _ = fs::remove_file(wrapper_path);

    // Run trace
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

    // Modify policy to allow some paths (simulating review-ui)
    setup_policy_with_ro(policy_path, &["/bin", "/lib"]).expect("Failed to setup policy");

    // Run create
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

    // Verify the script contains expected paths
    let policy_data = fs::read_to_string(policy_path).expect("Failed to read policy");
    assert!(
        policy_data.contains("ReadOnly"),
        "Policy should have ReadOnly entries"
    );

    // Clean up
    let _ = fs::remove_file(policy_path);
    let _ = fs::remove_file(wrapper_path);
}

#[test]
fn test_trace_produces_valid_json() {
    let bin_path = get_bin_path();
    let policy_path = "/tmp/myjail_test_valid.json";

    let _ = fs::remove_file(policy_path);

    let output = Command::new(&bin_path)
        .args(["trace", policy_path, "--", "ls"])
        .output()
        .expect("Failed to run trace");

    assert!(output.status.success());
    assert!(Path::new(policy_path).exists());

    // Verify it's valid JSON with expected structure
    let data = fs::read_to_string(policy_path).expect("Failed to read policy");
    let trees: Vec<serde_json::Value> = serde_json::from_str(&data).expect("Invalid JSON");
    assert!(!trees.is_empty());

    // Verify entries have expected fields
    if let Some(tree) = trees.first() {
        if let Some(entries) = tree.get("entries").and_then(|e| e.as_array()) {
            assert!(!entries.is_empty());
            if let Some(entry) = entries.first() {
                assert!(entry.get("path").is_some());
                assert!(entry.get("access").is_some());
            }
        }
    }

    let _ = fs::remove_file(policy_path);
}

#[test]
fn test_create_requires_existing_policy() {
    let bin_path = get_bin_path();

    let output = Command::new(&bin_path)
        .args(["create", "/nonexistent/path.json", "echo"])
        .output()
        .expect("Failed to run create");

    assert!(!output.status.success());
}

#[test]
fn test_create_with_nested_path() {
    let bin_path = get_bin_path();
    let policy_path = "/tmp/myjail_test_nested.json";

    let _ = fs::remove_file(policy_path);

    // Create policy with /nix path (simulating trace output without root)
    let policy = r#"[
  {
    "entries": []
  },
  {
    "entries": [
      {
        "path": "/nix",
        "access": "ReadOnly"
      }
    ]
  },
  {
    "entries": []
  }
]"#;

    fs::write(policy_path, policy).expect("Failed to write policy");

    let output = Command::new(&bin_path)
        .args(["create", policy_path, "/bin/sh"])
        .output()
        .expect("Failed to run create");

    assert!(output.status.success(), "create should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should include /nix, not collapse to /
    assert!(
        stdout.contains("--ro-bind /nix /nix"),
        "Output should contain --ro-bind /nix /nix, got: {}",
        stdout
    );

    // Should NOT have --ro-bind / / (unless / is explicitly in policy)
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
