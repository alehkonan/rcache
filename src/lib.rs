use rayon::prelude::*;
use std::env;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

/// Allocated size on disk (same idea as `du`), summed recursively.
/// Subdirectories are scanned in parallel via rayon's work-stealing pool.
fn dir_size(path: &Path) -> u64 {
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
    own + children.par_iter().map(|p| dir_size(p)).sum::<u64>()
}

/// Immediate children of `dir`, so each app gets its own row.
fn children_of(dir: &Path) -> Vec<PathBuf> {
    fs::read_dir(dir)
        .map(|rd| rd.filter_map(|e| e.ok()).map(|e| e.path()).collect())
        .unwrap_or_default()
}

/// Every cache location we know about that exists on this machine.
fn cache_targets(home: &Path) -> Vec<PathBuf> {
    // 1) Each app's folder inside the standard cache directories
    let mut targets: Vec<PathBuf> = Vec::new();
    targets.extend(children_of(&home.join("Library/Caches")));
    targets.extend(children_of(Path::new("/Library/Caches")));

    // 2) Well-known developer/tool caches that live elsewhere
    for rel in [
        "Library/Developer/Xcode/DerivedData",
        "Library/Developer/Xcode/iOS DeviceSupport",
        "Library/Developer/CoreSimulator",
        "Library/Logs",
        ".cache",
        ".npm",
        ".gradle/caches",
        ".cargo/registry",
        ".cocoapods",
    ] {
        targets.push(home.join(rel));
    }

    targets.retain(|p| p.exists());
    targets
}

/// Decimal units, matching Finder.
fn human(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1000.0 && i < UNITS.len() - 1 {
        v /= 1000.0;
        i += 1;
    }
    if i == 0 {
        format!("{} B", bytes)
    } else {
        format!("{:.1} {}", v, UNITS[i])
    }
}

/// Entry point for the CLI. Usage: rcache [-n TOP]
pub fn run() {
    let args: Vec<String> = env::args().collect();
    let top: usize = args
        .iter()
        .position(|a| a == "-n")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(30);

    let home = PathBuf::from(env::var("HOME").expect("HOME is not set"));
    let targets = cache_targets(&home);

    // Scan all targets in parallel (each one also parallelizes internally)
    let mut results: Vec<(PathBuf, u64)> = targets
        .par_iter()
        .map(|p| (p.clone(), dir_size(p)))
        .collect();

    results.sort_by(|a, b| b.1.cmp(&a.1));
    let total: u64 = results.iter().map(|r| r.1).sum();

    let home_str = home.to_string_lossy().to_string();
    for (path, size) in results.iter().take(top) {
        let shown = path.to_string_lossy().replacen(&home_str, "~", 1);
        println!("{:>10}  {}", human(*size), shown);
    }
    println!("{:>10}  TOTAL ({} locations)", human(total), results.len());
}
