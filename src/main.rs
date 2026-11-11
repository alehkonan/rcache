//! Command-line entry point: parses arguments, runs the scan, prints the report.

use clap::Parser;
use rcache::report::{self, Options};
use std::env;
use std::io::{self, BufWriter, IsTerminal, Write};
use std::path::PathBuf;
use std::process::ExitCode;

/// Find what is using disk space in macOS caches, with an explanation of
/// what each cache is, whether it is safe to delete, and how to clean it.
#[derive(Parser)]
#[command(version)]
struct Cli {
    /// Show the N largest caches.
    #[arg(short = 'n', long = "top", value_name = "N", default_value_t = 30)]
    top: usize,

    /// Show every cache found (overrides --top).
    #[arg(short, long)]
    all: bool,

    /// One line per cache, without explanations.
    #[arg(short, long)]
    brief: bool,
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    let Some(home) = env::var_os("HOME").filter(|h| !h.is_empty()).map(PathBuf::from) else {
        eprintln!("rcache: HOME is not set");
        return ExitCode::FAILURE;
    };

    let findings = rcache::find_caches(&home);

    let stdout = io::stdout();
    let options = Options {
        limit: (!cli.all).then_some(cli.top),
        explain: !cli.brief,
        color: stdout.is_terminal() && env::var_os("NO_COLOR").is_none(),
    };
    let mut out = BufWriter::new(stdout.lock());
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
