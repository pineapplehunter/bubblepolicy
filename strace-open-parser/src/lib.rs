use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Access {
    Deny,
    ReadOnly,
    ReadWrite,
    Tmpfs,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileAccess {
    pub path: String,
    pub access: Access,
}

pub fn parse_strace_output(input: &str) -> Vec<FileAccess> {
    let re = Regex::new(r#"^\d+\s+(?:open\("[^"]+"\s*,\s*(O_[A-Z_|]+)|openat\([^,]+,\s*"[^"]+"\s*,\s*(O_[A-Z_|]+))"#)
        .unwrap();
    input
        .lines()
        .filter_map(|line| {
            re.captures(line).map(|caps| {
                let path_re = Regex::new(r#""([^"]+)""#).unwrap();
                let path = path_re
                    .captures(line)
                    .and_then(|c| c.get(1))
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default();
                let flags = caps
                    .get(1)
                    .or(caps.get(2))
                    .map(|m| m.as_str())
                    .unwrap_or("");
                let access = if flags.contains("RDONLY")
                    && !flags.contains("WRONLY")
                    && !flags.contains("RDWR")
                {
                    Access::ReadOnly
                } else {
                    Access::ReadWrite
                };
                FileAccess { path, access }
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openat_rdonly() {
        let r = parse_strace_output("12345 openat(AT_FDCWD, \"/etc/passwd\", O_RDONLY) = 3");
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].path, "/etc/passwd");
        assert_eq!(r[0].access, Access::ReadOnly);
    }

    #[test]
    fn test_open_rdonly() {
        let r = parse_strace_output("12345 open(\"/etc/passwd\", O_RDONLY) = 3");
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].path, "/etc/passwd");
        assert_eq!(r[0].access, Access::ReadOnly);
    }

    #[test]
    fn test_open_rdwr() {
        let r = parse_strace_output("12345 open(\"/tmp/file\", O_RDWR) = 3");
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].path, "/tmp/file");
        assert_eq!(r[0].access, Access::ReadWrite);
    }

    #[test]
    fn test_openat_wronly_with_flags() {
        let r = parse_strace_output(
            "12345 openat(AT_FDCWD, \"/var/log/app.log\", O_WRONLY|O_CREAT) = 3",
        );
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].path, "/var/log/app.log");
        assert_eq!(r[0].access, Access::ReadWrite);
    }

    #[test]
    fn test_multiple_lines() {
        let r = parse_strace_output(
            "12345 open(\"/etc/passwd\", O_RDONLY) = 3\n67890 open(\"/tmp/file\", O_RDWR) = 4",
        );
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].path, "/etc/passwd");
        assert_eq!(r[0].access, Access::ReadOnly);
        assert_eq!(r[1].path, "/tmp/file");
        assert_eq!(r[1].access, Access::ReadWrite);
    }
}
