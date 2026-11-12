// SPDX-License-Identifier: AGPL-3.0-only
/**
 * Debug patch for differential tests against `ofsim` built while executions are missing: makes
 * every execution the Rust engine has not ported inert in the upstream engine too (run
 * `ofsim` with `OFSIM_INERT=1` and the tracer with `--inert 1`). Traces made this way never match
 * the record hashes; they only compare the two engines with each other.
 */
import { DoomsdayClockExecution } from "../src/core/execution/DoomsdayClockExecution";
import { Executor } from "../src/core/execution/ExecutionManager";
import { MirvExecution } from "../src/core/execution/MIRVExecution";
import { NationExecution } from "../src/core/execution/NationExecution";
import { NoOpExecution } from "../src/core/execution/NoOpExecution";
import { NukeExecution } from "../src/core/execution/NukeExecution";
import { SAMLauncherExecution } from "../src/core/execution/SAMLauncherExecution";
import { TradeShipExecution } from "../src/core/execution/TradeShipExecution";
import { TribeExecution } from "../src/core/execution/TribeExecution";
import { WarshipExecution } from "../src/core/execution/WarshipExecution";

/** Intents whose executions are ported on the Rust side; extend as executions land. */
const PORTED_INTENTS = new Set(["spawn", "emoji", "quick_chat", "mark_disconnected", "toggle_pause"]);
/** Extra ported intent types for a work-in-progress branch (comma list in `OFSIM_PORTED_INTENTS`). */
for (const t of (process.env.OFSIM_PORTED_INTENTS ?? "").split(",")) if (t) PORTED_INTENTS.add(t);
/** Engine-created executions kept active (comma list in `OFSIM_PORTED_EXECS`: Nation, Tribe, DoomsdayClock). */
const PORTED_EXECS = new Set((process.env.OFSIM_PORTED_EXECS ?? "").split(",").filter((t) => t));

/** Executions created from inside ported ones that are made inert (comma list in `OFSIM_INERT_EXECS`: Nuke, Mirv, SAMLauncher, TradeShip, Warship). */
const INERT_EXECS = new Set((process.env.OFSIM_INERT_EXECS ?? "").split(",").filter((t) => t));

export function applyInertPatch(): void {
  const original = Executor.prototype.createExec;
  Executor.prototype.createExec = function (intent) {
    const exec = original.call(this, intent);
    return PORTED_INTENTS.has(intent.type) ? exec : new NoOpExecution();
  };
  const nested = [
    ["Nuke", NukeExecution],
    ["Mirv", MirvExecution],
    ["SAMLauncher", SAMLauncherExecution],
    ["TradeShip", TradeShipExecution],
    ["Warship", WarshipExecution],
  ] as const;
  for (const [name, cls] of nested) {
    if (!INERT_EXECS.has(name)) continue;
    (cls.prototype as { init: () => void }).init = () => {};
    (cls.prototype as { tick: () => void }).tick = () => {};
  }
  const inert = [
    ["Nation", NationExecution],
    ["Tribe", TribeExecution],
    ["DoomsdayClock", DoomsdayClockExecution],
  ] as const;
  for (const [name, cls] of inert) {
    if (PORTED_EXECS.has(name)) continue;
    (cls.prototype as { init: () => void }).init = () => {};
    (cls.prototype as { tick: () => void }).tick = () => {};
  }
}
