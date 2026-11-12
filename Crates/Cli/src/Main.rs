// SPDX-License-Identifier: AGPL-3.0-only
//! `ofsim`: command-line replayer for OpenFront game records (unofficial port, see NOTICE).

mod Naval;
mod NukeScenario;
mod Scenario;
mod Trace;

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
        /// Upstream clone, read for the maps of builds whose map files differ from the baseline
        #[arg(long, default_value = "Cache/OpenFrontIO")]
        upstream: PathBuf,
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
        /// Upstream clone, read for the maps of builds whose map files differ from the baseline
        #[arg(long, default_value = "Cache/OpenFrontIO")]
        upstream: PathBuf,
        /// Output NDJSON file
        #[arg(long)]
        out: PathBuf,
        /// Stop after this tick (inclusive)
        #[arg(long)]
        until: Option<u32>,
    },
    /// Seeded territory fuzz for differential tests (see Reference/ScenarioDump.ts)
    #[command(hide = true)]
    Scenario {
        /// Game record (.json.gz)
        #[arg(long)]
        record: PathBuf,
        /// Directory of upstream map resources
        #[arg(long)]
        maps: PathBuf,
        /// Output NDJSON file
        #[arg(long)]
        out: PathBuf,
        /// Ticks of the record to run first
        #[arg(long)]
        warm: u32,
        /// Number of fuzz rounds (one tick each)
        #[arg(long)]
        rounds: u32,
        /// Territory mutations per round
        #[arg(long)]
        steps: u32,
        /// Mutate only the first K alive players (0 = all)
        #[arg(long, default_value_t = 0)]
        focus: u32,
        /// Only take unowned land
        #[arg(long, default_value_t = false)]
        free_only: bool,
        /// Players the first alive player walls in before the fuzz
        #[arg(long, default_value_t = 0)]
        enclaves: u32,
        /// Manhattan radius of the wall
        #[arg(long, default_value_t = 8)]
        radius: i32,
    },
    /// Scripted port, warship and boat scenario for differential tests (see Reference/NavalScenarioDump.ts)
    #[command(hide = true)]
    NavalScenario {
        /// Game record (.json.gz)
        #[arg(long)]
        record: PathBuf,
        /// Directory of upstream map resources
        #[arg(long)]
        maps: PathBuf,
        /// Output NDJSON file
        #[arg(long)]
        out: PathBuf,
        /// Ticks of the record to run first
        #[arg(long)]
        warm: u32,
        /// Ticks to run after the setup
        #[arg(long)]
        ticks: u32,
        /// Number of players that get a port and a warship
        #[arg(long, default_value_t = 6)]
        players: usize,
    },
    /// Seeded nuke, SAM and doomsday program for differential tests (see Reference/NukeScenarioDump.ts)
    #[command(hide = true)]
    NukeScenario {
        /// Game record (.json.gz)
        #[arg(long)]
        record: PathBuf,
        /// Directory of upstream map resources
        #[arg(long)]
        maps: PathBuf,
        /// Output NDJSON file (a `.digest` file is written next to it)
        #[arg(long)]
        out: PathBuf,
        /// Ticks of the record to run first
        #[arg(long)]
        warm: u32,
        /// Number of launch rounds
        #[arg(long)]
        rounds: u32,
        /// Ticks per round
        #[arg(long, default_value_t = 40)]
        spacing: u32,
        /// Ticks after the last round
        #[arg(long, default_value_t = 150)]
        tail: u32,
        /// Doomsday clock speed (slow, normal, fast, veryfast); off when absent
        #[arg(long)]
        doomsday: Option<String>,
        /// Number of launching players
        #[arg(long, default_value_t = 4)]
        attackers: usize,
        /// Manhattan radius of the grow phase around each spawn
        #[arg(long, default_value_t = 25)]
        grow: i32,
        /// One in this many players gets a SAM launcher (0 = none)
        #[arg(long, default_value_t = 25)]
        sam_mod: usize,
    },
    /// Replay a record and write state snapshots
    Replay {
        /// Game record (.json.gz)
        #[arg(long)]
        record: PathBuf,
        /// Directory of upstream map resources
        #[arg(long)]
        maps: PathBuf,
        /// Upstream clone, read for the maps of builds whose map files differ from the baseline
        #[arg(long, default_value = "Cache/OpenFrontIO")]
        upstream: PathBuf,
        /// Output directory
        #[arg(long)]
        out: PathBuf,
        /// Snapshot interval in ticks
        #[arg(long, default_value_t = 100)]
        every: u32,
    },
}

/// Maps root for a record: the given baseline maps, or the record's own build maps from `upstream`.
fn resolve_maps(
    record: &std::path::Path,
    maps: &std::path::Path,
    upstream: &std::path::Path,
) -> anyhow::Result<PathBuf> {
    let loaded = ofsim_record::Records::GameRecord::load_decompressed(record)?;
    ofsim_engine::Builds::Maps::maps_root_for(&loaded, maps, upstream)
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    ofsim_engine::Runner::Replay::silence_panics();
    match cli.command {
        Command::Verify {
            record,
            maps,
            upstream,
            until,
        } => {
            let maps = match resolve_maps(&record, &maps, &upstream) {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("ofsim verify: {e:#}");
                    return ExitCode::from(2);
                }
            };
            let name = record
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let name = name
                .trim_end_matches(".gz")
                .trim_end_matches(".json")
                .to_string();
            match ofsim_engine::Runner::Replay::verify(&record, &maps, until) {
                Ok(report) => {
                    println!("{}", report.line(&name));
                    match report.verdict {
                        ofsim_engine::Runner::Replay::Verdict::Ok
                        | ofsim_engine::Runner::Replay::Verdict::Partial => ExitCode::SUCCESS,
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
            upstream,
            out,
            until,
        } => match resolve_maps(&record, &maps, &upstream)
            .and_then(|maps| Trace::trace(&record, &maps, &out, until))
        {
            Ok(summary) => {
                eprintln!("{summary}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("ofsim trace: {e:#}");
                ExitCode::from(2)
            }
        },
        Command::Scenario {
            record,
            maps,
            out,
            warm,
            rounds,
            steps,
            focus,
            free_only,
            enclaves,
            radius,
        } => match Scenario::scenario(
            &record, &maps, &out, warm, rounds, steps, focus, free_only, enclaves, radius,
        ) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("ofsim scenario: {e:#}");
                ExitCode::from(2)
            }
        },
        Command::NavalScenario {
            record,
            maps,
            out,
            warm,
            ticks,
            players,
        } => match Naval::naval_scenario(&record, &maps, &out, warm, ticks, players) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("ofsim naval-scenario: {e:#}");
                ExitCode::from(2)
            }
        },
        Command::NukeScenario {
            record,
            maps,
            out,
            warm,
            rounds,
            spacing,
            tail,
            doomsday,
            attackers,
            grow,
            sam_mod,
        } => match NukeScenario::nuke_scenario(&NukeScenario::NukeScenarioArgs {
            record: &record,
            maps: &maps,
            out: &out,
            warm,
            rounds,
            spacing,
            tail,
            doomsday: doomsday.as_deref(),
            attackers,
            grow,
            sam_mod,
        }) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("ofsim nuke-scenario: {e:#}");
                ExitCode::from(2)
            }
        },
        Command::Replay { .. } => {
            eprintln!("ofsim replay: not implemented yet");
            ExitCode::from(2)
        }
    }
}
