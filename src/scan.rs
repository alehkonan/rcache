//! Disk-usage measurement: allocated bytes on disk, the same idea as `du`.

use rayon::prelude::*;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

/// Allocated size on disk (same idea as `du`), summed recursively.
/// Subdirectories are scanned in parallel via rayon's work-stealing pool.
pub fn disk_usage(path: &Path) -> u64 {
    let meta = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(_) => return 0, // permission denied, vanished file, etc.
    };
    if meta.file_type().is_symlink() {
        return 0; // never follow symlinks
    }
    let own = meta.blocks() * 512; // st_blocks is always in 512-byte units
    if !meta.is_dir() {
        return own;
    }
    let entries = match fs::read_dir(path) {
        Ok(e) => e,
        Err(_) => return own,
    };
    let children: Vec<PathBuf> = entries.filter_map(|e| e.ok()).map(|e| e.path()).collect();
    own + children.par_iter().map(|p| disk_usage(p)).sum::<u64>()
}
