// SPDX-License-Identifier: AGPL-3.0-only
/**
 * Debug patch for differential tests against `ofsim` built while executions are missing: makes
 * every execution the Rust engine has not ported inert in the upstream engine too (run
 * `ofsim` with `OFSIM_INERT=1` and the tracer with `--inert 1`). Traces made this way never match
 * the record hashes; they only compare the two engines with each other.
 */
import { AttackExecution } from "../src/core/execution/AttackExecution";
import { DoomsdayClockExecution } from "../src/core/execution/DoomsdayClockExecution";
import { Executor } from "../src/core/execution/ExecutionManager";
import { NationExecution } from "../src/core/execution/NationExecution";
import { NoOpExecution } from "../src/core/execution/NoOpExecution";
import { ShellExecution } from "../src/core/execution/ShellExecution";
import { TribeExecution } from "../src/core/execution/TribeExecution";

/** Intents whose executions are ported on the Rust side; extend as executions land. */
const PORTED_INTENTS = new Set(["spawn", "emoji", "quick_chat", "mark_disconnected", "toggle_pause"]);
/** Extra ported intent types for a work-in-progress branch (comma list in `OFSIM_PORTED_INTENTS`). */
for (const t of (process.env.OFSIM_PORTED_INTENTS ?? "").split(",")) if (t) PORTED_INTENTS.add(t);
/** Engine-created executions kept active (comma list in `OFSIM_PORTED_EXECS`: Nation, Tribe, DoomsdayClock). */
const PORTED_EXECS = new Set((process.env.OFSIM_PORTED_EXECS ?? "").split(",").filter((t) => t));

/** Executions created from inside ported ones, made inert on request (comma list in `OFSIM_INERT_CLASSES`: Attack, Shell). */
const INERT_CLASSES = new Set((process.env.OFSIM_INERT_CLASSES ?? "").split(",").filter((t) => t));

export function applyInertPatch(): void {
  const original = Executor.prototype.createExec;
  Executor.prototype.createExec = function (intent) {
    const exec = original.call(this, intent);
    return PORTED_INTENTS.has(intent.type) ? exec : new NoOpExecution();
  };
  const inert = [
    ["Nation", NationExecution],
    ["Tribe", TribeExecution],
    ["DoomsdayClock", DoomsdayClockExecution],
  ] as const;
  const created = [
    ["Attack", AttackExecution],
    ["Shell", ShellExecution],
  ] as const;
  for (const [name, cls] of created) {
    if (!INERT_CLASSES.has(name)) continue;
    (cls.prototype as { init: () => void }).init = () => {};
    (cls.prototype as { tick: () => void }).tick = () => {};
  }
  for (const [name, cls] of inert) {
    if (PORTED_EXECS.has(name)) continue;
    (cls.prototype as { init: () => void }).init = () => {};
    (cls.prototype as { tick: () => void }).tick = () => {};
  }
}
