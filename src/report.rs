//! Rendering findings as text.

use crate::Finding;
use std::io::{self, Write};

pub struct Options {
    /// Show at most this many rows (the largest ones).
    pub limit: usize,
}

pub fn render(out: &mut impl Write, findings: &[Finding], opts: &Options) -> io::Result<()> {
    for f in findings.iter().take(opts.limit) {
        writeln!(out, "{:>10}  {}", human(f.size), f.display)?;
    }
    let total: u64 = findings.iter().map(|f| f.size).sum();
    writeln!(out, "{:>10}  TOTAL ({} locations)", human(total), findings.len())
}

/// Decimal units, matching Finder.
pub fn human(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_sizes() {
        assert_eq!(human(0), "0 B");
        assert_eq!(human(999), "999 B");
        assert_eq!(human(1_500), "1.5 KB");
        assert_eq!(human(9_900_000_000), "9.9 GB");
    }
}
