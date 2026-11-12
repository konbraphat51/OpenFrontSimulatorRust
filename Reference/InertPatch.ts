// SPDX-License-Identifier: AGPL-3.0-only
/**
 * Debug patch for differential tests against `ofsim` built while executions are missing: makes
 * every execution the Rust engine has not ported inert in the upstream engine too (run
 * `ofsim` with `OFSIM_INERT=1` and the tracer with `--inert 1`). Traces made this way never match
 * the record hashes; they only compare the two engines with each other.
 */
import { DoomsdayClockExecution } from "../src/core/execution/DoomsdayClockExecution";
import { Executor } from "../src/core/execution/ExecutionManager";
import { NationExecution } from "../src/core/execution/NationExecution";
import { NoOpExecution } from "../src/core/execution/NoOpExecution";
import { TribeExecution } from "../src/core/execution/TribeExecution";

/** Intents whose executions are ported on the Rust side; extend as executions land. */
const PORTED_INTENTS = new Set(["spawn", "emoji", "quick_chat", "mark_disconnected", "toggle_pause"]);

export function applyInertPatch(): void {
  const original = Executor.prototype.createExec;
  Executor.prototype.createExec = function (intent) {
    const exec = original.call(this, intent);
    return PORTED_INTENTS.has(intent.type) ? exec : new NoOpExecution();
  };
  for (const cls of [NationExecution, TribeExecution, DoomsdayClockExecution]) {
    (cls.prototype as { init: () => void }).init = () => {};
    (cls.prototype as { tick: () => void }).tick = () => {};
  }
}
