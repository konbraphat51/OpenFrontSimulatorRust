// SPDX-License-Identifier: AGPL-3.0-only
//! `ofsim`: command-line replayer for OpenFront game records (unofficial port, see NOTICE).

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
    let name = match cli.command {
        Command::Verify { .. } => "verify",
        Command::Replay { .. } => "replay",
    };
    eprintln!("ofsim {name}: not implemented yet");
    ExitCode::from(2)
}
