// SPDX-License-Identifier: AGPL-3.0-only
//! `ofsim`: command-line replayer for OpenFront game records (unofficial port, see NOTICE).

mod JsNumberFormat;
mod Replaying;
mod Trace;
mod Verify;

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;

const ABOUT: &str = "OfSimRust: unofficial replay simulator for OpenFront game records. Not affiliated with or endorsed by OpenFront Inc.";

const LONG_VERSION: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    "\nOfSimRust: unofficial replay simulator for OpenFront game records. © OpenFront and Contributors.",
    "\nRust port © OfSimRust contributors.",
    "\nAGPL-3.0-only; source: https://github.com/konbraphat51/OfSimRust"
);

const AFTER_HELP: &str = "OfSimRust: unofficial replay simulator for OpenFront game records. © OpenFront and Contributors.\n\
Rust port © OfSimRust contributors.\n\
AGPL-3.0-only; source: https://github.com/konbraphat51/OfSimRust";

/// Command-line arguments of `ofsim`.
#[derive(Parser)]
#[command(name = "ofsim", version, long_version = LONG_VERSION, about = ABOUT, after_help = AFTER_HELP)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

/// Subcommands of `ofsim`.
#[derive(Subcommand)]
enum Command {
    /// Verify a record against its stored state hashes
    Verify {
        /// Game record (.json.gz)
        #[arg(long)]
        record: PathBuf,
        /// Directory of upstream map resources
        #[arg(long)]
        maps: PathBuf,
        /// Stop after this tick (inclusive)
        #[arg(long)]
        until: Option<u32>,
    },
    /// Write the per-tick NDJSON trace of a record (format of Reference/README.md)
    Trace {
        /// Game record (.json.gz)
        #[arg(long)]
        record: PathBuf,
        /// Directory of upstream map resources
        #[arg(long)]
        maps: PathBuf,
        /// Output NDJSON file
        #[arg(long)]
        out: PathBuf,
        /// Stop after this tick (inclusive)
        #[arg(long)]
        until: Option<u32>,
    },
    /// Replay a record and write state snapshots
    Replay {
        /// Game record (.json.gz)
        #[arg(long)]
        record: PathBuf,
        /// Directory of upstream map resources
        #[arg(long)]
        maps: PathBuf,
        /// Output directory
        #[arg(long)]
        out: PathBuf,
        /// Snapshot interval in ticks
        #[arg(long, default_value_t = 100)]
        every: u32,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    Replaying::silence_panics();
    match cli.command {
        Command::Verify {
            record,
            maps,
            until,
        } => {
            let name = record
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let name = name
                .trim_end_matches(".gz")
                .trim_end_matches(".json")
                .to_string();
            match Verify::verify(&record, &maps, until) {
                Ok(report) => {
                    println!("{}", report.line(&name));
                    match report.verdict {
                        Verify::Verdict::Ok | Verify::Verdict::Partial => ExitCode::SUCCESS,
                        _ => ExitCode::from(1),
                    }
                }
                Err(e) => {
                    eprintln!("ofsim verify: {e:#}");
                    ExitCode::from(2)
                }
            }
        }
        Command::Trace {
            record,
            maps,
            out,
            until,
        } => match Trace::trace(&record, &maps, &out, until) {
            Ok(summary) => {
                eprintln!("{summary}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("ofsim trace: {e:#}");
                ExitCode::from(2)
            }
        },
        Command::Replay { .. } => {
            eprintln!("ofsim replay: not implemented yet");
            ExitCode::from(2)
        }
    }
}
