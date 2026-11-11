//! Command-line entry point: parses arguments, runs the scan, prints the report.

use clap::Parser;
use rcache::report::{self, Options};
use std::env;
use std::io::{self, BufWriter, Write};
use std::path::PathBuf;
use std::process::ExitCode;

/// Find what is using disk space in macOS caches.
#[derive(Parser)]
#[command(version)]
struct Cli {
    /// Show the N largest caches.
    #[arg(short = 'n', long = "top", value_name = "N", default_value_t = 30)]
    top: usize,
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    let Some(home) = env::var_os("HOME").filter(|h| !h.is_empty()).map(PathBuf::from) else {
        eprintln!("rcache: HOME is not set");
        return ExitCode::FAILURE;
    };

    let findings = rcache::find_caches(&home);

    let options = Options { limit: cli.top };
    let mut out = BufWriter::new(io::stdout().lock());
    match report::render(&mut out, &findings, &options).and_then(|()| out.flush()) {
        Ok(()) => ExitCode::SUCCESS,
        // e.g. `rcache | head`: the reader went away, which is not an error.
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("rcache: {e}");
            ExitCode::FAILURE
        }
    }
}
