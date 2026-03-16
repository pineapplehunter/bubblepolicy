# Testing myjail

This document describes how to test the myjail tool using the provided convenience script.

## Quick Test

Run the full test suite with:

```bash
./test_myjail.sh test
```

## Available Commands

| Command | Description |
|---------|-------------|
| `./test_myjail.sh build` | Build the project |
| `./test_myjail.sh test` | Run all tests |
| `./test_myjail.sh clean` | Clean test files |
| `./test_myjail.sh all` | Build and run all tests |

### Individual Tests

| Command | Description |
|---------|-------------|
| `./test_myjail.sh trace` | Test trace command only |
| `./test_myjail.sh review` | Test review command only |
| `./test_myjail.sh create` | Test create command only |
| `./test_myjail.sh grouping` | Test directory grouping optimization |
| `./test_myjail.sh merge` | Test multi-file merge in review |

## Test Coverage

The test script covers:

1. **Trace Command**
   - Tracing a simple command (echo)
   - Verifying trace file creation
   - Counting files accessed

2. **Review Command**
   - Generating policy from trace
   - Verifying policy file creation
   - Counting policy entries

3. **Create Command**
   - Creating wrapper script from policy
   - Verifying wrapper file creation
   - Counting bind mounts generated

4. **Directory Grouping**
   - Tracing a command with multiple files in same directories
   - Verifying that directories are grouped instead of individual files
   - Comparing bind mount count vs file count

5. **Multi-file Merge**
   - Creating multiple trace files
   - Merging them in review command
   - Verifying deduplication works correctly

## Manual Testing

### Trace to Review to Create Workflow

```bash
# Step 1: Trace
./target/release/myjail trace --output /tmp/trace.json -- echo "hello"

# Step 2: Review (generate policy)
./target/release/myjail review --generate-policy --output /tmp/policy.json /tmp/trace.json

# Step 3: Create wrapper
./target/release/myjail create --policy /tmp/policy.json /bin/echo --output /tmp/echo_wrapper.sh

# Step 4: Make executable and test
chmod +x /tmp/echo_wrapper.sh
/tmp/echo_wrapper.sh
```

### Test Directory Grouping

```bash
# Trace a command that accesses many files in same directories
./target/release/myjail trace --output /tmp/trace_multi.json -- ls -la /etc

# Generate policy and wrapper
./target/release/myjail review --generate-policy --output /tmp/policy_multi.json /tmp/trace_multi.json
./target/release/myjail create --policy /tmp/policy_multi.json /bin/ls --output /tmp/ls_wrapper.sh

# Compare number of files vs bind mounts
echo "Files in policy: $(grep -c '"path"' /tmp/policy_multi.json)"
echo "Bind mounts in wrapper: $(grep -c "ro-bind" /tmp/ls_wrapper.sh)"
```

## Test Files Location

All test files are created in `/tmp/myjail_test/` directory:
- `trace1.json` - Trace output
- `policy1.json` - Generated policy
- `echo_wrapper.sh` - Generated wrapper script
- `trace_multi.json` - Multi-file trace
- `ls_wrapper.sh` - Wrapper with directory grouping

## Troubleshooting

### Build fails
```bash
# Clean and rebuild
cargo clean
cargo build --release
```

### strace not found
```bash
# Install strace (Ubuntu/Debian)
sudo apt-get install strace

# Install strace (NixOS)
nix-env -iA nixos.strace
```

### bwrap not found
```bash
# Install bubblewrap (Ubuntu/Debian)
sudo apt-get install bubblewrap

# Install bubblewrap (NixOS)
nix-env -iA nixos.bubblewrap
```

## Performance Comparison

Directory grouping can significantly reduce the number of bind mounts:

| Scenario | Files in Policy | Bind Mounts (Before) | Bind Mounts (After) | Reduction |
|----------|-----------------|---------------------|---------------------|-----------|
| Simple echo | 19 | 19 | 9 | 53% |
| ls /etc | 38 | 38 | 20 | 47% |

The grouping algorithm identifies common parent directories and binds them instead of individual files, making the generated wrapper scripts more efficient and maintainable.
