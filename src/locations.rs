//! Where caches live on macOS.
//!
//! Paths starting with `~/` are relative to the user's home directory.

use std::fs;
use std::path::{Path, PathBuf};

/// Directories shared by many apps/tools: each immediate child gets its own row.
pub const SHARED: &[&str] = &["~/Library/Caches", "/Library/Caches"];

/// Single-purpose cache directories, reported as one row each.
/// None of them may live inside a [`SHARED`] directory, or it would be counted twice.
pub const DEDICATED: &[&str] = &[
    "~/Library/Developer/Xcode/DerivedData",
    "~/Library/Developer/Xcode/iOS DeviceSupport",
    "~/Library/Developer/CoreSimulator",
    "~/Library/Logs",
    "~/.cache",
    "~/.npm",
    "~/.gradle/caches",
    "~/.cargo/registry",
    "~/.cocoapods",
];

/// Every cache location from [`SHARED`] and [`DEDICATED`] that exists on this machine.
pub fn discover(home: &Path) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = SHARED
        .iter()
        .flat_map(|dir| children(&resolve(home, dir)))
        .collect();
    found.extend(DEDICATED.iter().map(|rel| resolve(home, rel)));
    // `symlink_metadata` so a dangling symlink still counts as "exists" (as 0 bytes).
    found.retain(|p| fs::symlink_metadata(p).is_ok());
    found
}

/// Expands a leading `~/` to `home`.
pub fn resolve(home: &Path, path: &str) -> PathBuf {
    match path.strip_prefix("~/") {
        Some(rest) => home.join(rest),
        None => PathBuf::from(path),
    }
}

/// `path` as shown to users, with the home directory abbreviated to `~`.
pub fn display(home: &Path, path: &Path) -> String {
    match path.strip_prefix(home) {
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.display().to_string(),
    }
}

fn children(dir: &Path) -> Vec<PathBuf> {
    fs::read_dir(dir)
        .map(|entries| entries.flatten().map(|e| e.path()).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dedicated_locations_do_not_overlap() {
        let all: Vec<&str> = SHARED.iter().chain(DEDICATED).copied().collect();
        for a in &all {
            for b in &all {
                if a != b {
                    assert!(!Path::new(a).starts_with(b), "{a} is inside {b} and would be counted twice");
                }
            }
        }
    }

    #[test]
    fn abbreviates_home() {
        let home = Path::new("/Users/me");
        assert_eq!(display(home, Path::new("/Users/me/.npm")), "~/.npm");
        assert_eq!(display(home, Path::new("/Users/meme/x")), "/Users/meme/x");
        assert_eq!(display(home, Path::new("/Library/Caches/x")), "/Library/Caches/x");
        assert_eq!(resolve(home, "~/.npm"), Path::new("/Users/me/.npm"));
    }
}
