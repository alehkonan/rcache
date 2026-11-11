//! Finds what is using disk space in macOS cache locations.
//!
//! The binary (`src/main.rs`) only parses arguments and prints; everything else
//! lives in this library:
//!
//! * [`locations`] where caches live,
//! * [`scan`] measuring their size on disk,
//! * [`report`] rendering results as text.

pub mod locations;
pub mod report;
pub mod scan;

use rayon::prelude::*;
use std::path::{Path, PathBuf};

/// One cache location and its size on disk.
pub struct Finding {
    pub path: PathBuf,
    /// `path` with the home directory abbreviated to `~`.
    pub display: String,
    /// Allocated bytes on disk.
    pub size: u64,
}

/// Scans every known cache location under `home`, largest first.
/// Empty or unreadable locations are left out.
pub fn find_caches(home: &Path) -> Vec<Finding> {
    let mut findings: Vec<Finding> = locations::discover(home)
        .into_par_iter()
        .filter_map(|path| {
            let size = scan::disk_usage(&path);
            if size == 0 {
                return None;
            }
            let display = locations::display(home, &path);
            Some(Finding { path, display, size })
        })
        .collect();
    findings.sort_unstable_by(|a, b| b.size.cmp(&a.size).then_with(|| a.display.cmp(&b.display)));
    findings
}
