//! Rendering findings as text.

use crate::Finding;
use crate::catalog::{Clean, Safety};
use std::io::{self, Write};

pub struct Options {
    /// Show at most this many rows (the largest ones); `None` shows all.
    pub limit: Option<usize>,
    /// Print what/safety/clean details under each row.
    pub explain: bool,
    /// Use ANSI colors.
    pub color: bool,
}

const INDENT: &str = "            ";

pub fn render(out: &mut impl Write, findings: &[Finding], opts: &Options) -> io::Result<()> {
    let style = Style(opts.color);
    let shown = opts.limit.map_or(findings.len(), |n| n.min(findings.len()));

    for f in &findings[..shown] {
        let size = human(f.size);
        let safety = f.info.safety;
        if opts.explain {
            writeln!(out, "{}  {}", style.bold(&format!("{size:>10}")), style.bold(&f.display))?;
            writeln!(out, "{INDENT}{}  {}", style.dim("What: "), f.info.what)?;
            let verdict = match safety {
                Safety::Safe => "yes",
                Safety::Caution => "caution",
                Safety::Keep => "no",
            };
            writeln!(out, "{INDENT}{}  {}: {}", style.dim("Safe? "), style.safety(safety, verdict), f.info.why)?;
            writeln!(out, "{INDENT}{}  {}", style.dim("Clean:"), clean_command(f.info.clean, &f.display))?;
            writeln!(out)?;
        } else {
            let label = format!("{:<7}", safety.label());
            writeln!(out, "{size:>10}  {}  {}", style.safety(safety, &label), f.display)?;
        }
    }

    let total: u64 = findings.iter().map(|f| f.size).sum();
    let safe: u64 = findings.iter().filter(|f| f.info.safety == Safety::Safe).map(|f| f.size).sum();
    let mut summary = format!(
        "{:>10}  total in {} locations, {} of it safe to clean",
        human(total),
        findings.len(),
        human(safe)
    );
    if shown < findings.len() {
        summary += &format!(" (largest {shown} shown, use --all to list every one)");
    }
    writeln!(out, "{}", style.bold(&summary))
}

/// Shell command (or instruction) that reclaims the space of one location.
pub fn clean_command(clean: Clean, display_path: &str) -> String {
    let sudo = if display_path.starts_with("~/") { "" } else { "sudo " };
    match clean {
        Clean::Remove => format!("{sudo}rm -rf {}", shell_path(display_path)),
        Clean::RemoveContents => format!("{sudo}rm -rf {}/*", shell_path(display_path)),
        Clean::Command(cmd) => cmd.to_string(),
    }
}

/// Quotes a path for the shell while keeping a leading `~/` expandable.
fn shell_path(path: &str) -> String {
    let (prefix, rest) = match path.strip_prefix("~/") {
        Some(rest) => ("~/", rest),
        None => ("", path),
    };
    let plain = |b: u8| b.is_ascii_alphanumeric() || b"/._-+@%:,".contains(&b);
    if rest.bytes().all(plain) {
        format!("{prefix}{rest}")
    } else {
        format!("{prefix}'{}'", rest.replace('\'', r"'\''"))
    }
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

/// ANSI styling that is a no-op when color is off.
struct Style(bool);

impl Style {
    fn paint(&self, code: &str, text: &str) -> String {
        if self.0 { format!("\x1b[{code}m{text}\x1b[0m") } else { text.to_string() }
    }
    fn bold(&self, text: &str) -> String {
        self.paint("1", text)
    }
    fn dim(&self, text: &str) -> String {
        self.paint("2", text)
    }
    fn safety(&self, safety: Safety, text: &str) -> String {
        let code = match safety {
            Safety::Safe => "32",
            Safety::Caution => "33",
            Safety::Keep => "31",
        };
        self.paint(code, text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog;
    use std::path::PathBuf;

    #[test]
    fn human_sizes() {
        assert_eq!(human(0), "0 B");
        assert_eq!(human(999), "999 B");
        assert_eq!(human(1_500), "1.5 KB");
        assert_eq!(human(9_900_000_000), "9.9 GB");
    }

    #[test]
    fn clean_commands_are_shell_safe() {
        assert_eq!(clean_command(Clean::Remove, "~/Library/Caches/pip"), "rm -rf ~/Library/Caches/pip");
        assert_eq!(clean_command(Clean::Remove, "~/Library/Caches/pgAdmin 4"), "rm -rf ~/'Library/Caches/pgAdmin 4'");
        assert_eq!(clean_command(Clean::RemoveContents, "~/Library/Logs"), "rm -rf ~/Library/Logs/*");
        assert_eq!(clean_command(Clean::Remove, "/Library/Caches/it's"), r"sudo rm -rf '/Library/Caches/it'\''s'");
    }

    #[test]
    fn renders_limit_and_summary() {
        let finding = |display: &str, size| Finding {
            path: PathBuf::from(display),
            display: display.to_string(),
            size,
            info: catalog::explain(display),
        };
        let findings = [finding("~/Library/Caches/Homebrew", 2_000), finding("~/Library/Caches/com.apple.akd", 1_000)];
        let mut out = Vec::new();
        render(&mut out, &findings, &Options { limit: Some(1), explain: false, color: false }).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert_eq!(
            text,
            "    2.0 KB  safe     ~/Library/Caches/Homebrew\n    \
             3.0 KB  total in 2 locations, 2.0 KB of it safe to clean (largest 1 shown, use --all to list every one)\n"
        );
    }
}
