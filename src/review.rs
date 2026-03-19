use color_eyre::{eyre::WrapErr, Result};
use log::info;
use std::fs;

use crate::common::{entries_to_string, parse_entries, Access, PolicyEntry};

pub fn run(
    file: &str,
    ro: &[String],
    rw: &[String],
    tmp: &[String],
    deny: &[String],
) -> Result<()> {
    let data =
        fs::read_to_string(file).with_context(|| format!("Failed to read file: {}", file))?;

    let mut entries = parse_entries(&data);

    for path in ro {
        set_access(&mut entries, path, Access::ReadOnly);
    }
    for path in rw {
        set_access(&mut entries, path, Access::ReadWrite);
    }
    for path in tmp {
        set_access(&mut entries, path, Access::Tmpfs);
    }
    for path in deny {
        set_access(&mut entries, path, Access::Deny);
    }

    let output = entries_to_string(&entries);

    fs::write(file, output).with_context(|| format!("Failed to write file: {}", file))?;

    info!("Updated: {}", file);
    Ok(())
}

fn set_access(entries: &mut Vec<PolicyEntry>, path: &str, access: Access) {
    if let Some(entry) = entries.iter_mut().find(|e| e.path == path) {
        entry.access = access;
    } else {
        entries.push(PolicyEntry {
            path: path.to_string(),
            access,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_access_existing() {
        let mut entries = vec![
            PolicyEntry {
                path: "/etc/passwd".to_string(),
                access: Access::ReadOnly,
            },
            PolicyEntry {
                path: "/etc/shadow".to_string(),
                access: Access::ReadOnly,
            },
        ];

        set_access(&mut entries, "/etc/passwd", Access::ReadWrite);

        assert_eq!(entries[0].access, Access::ReadWrite);
        assert_eq!(entries[1].access, Access::ReadOnly);
    }

    #[test]
    fn test_set_access_new() {
        let mut entries = vec![PolicyEntry {
            path: "/etc/passwd".to_string(),
            access: Access::ReadOnly,
        }];

        set_access(&mut entries, "/etc/shadow", Access::ReadWrite);

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[1].path, "/etc/shadow");
        assert_eq!(entries[1].access, Access::ReadWrite);
    }
}
