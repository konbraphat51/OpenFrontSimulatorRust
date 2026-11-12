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
import { AllianceExtensionExecution } from "../src/core/execution/alliance/AllianceExtensionExecution";
import { AllianceRequestExecution } from "../src/core/execution/alliance/AllianceRequestExecution";
import { ConstructionExecution } from "../src/core/execution/ConstructionExecution";
import { NationMIRVBehavior } from "../src/core/execution/nation/NationMIRVBehavior";
import { NationNukeBehavior } from "../src/core/execution/nation/NationNukeBehavior";
import { NationStructureBehavior } from "../src/core/execution/nation/NationStructureBehavior";
import { AiAttackBehavior } from "../src/core/execution/utils/AiAttackBehavior";

/** Intents whose executions are ported on the Rust side; extend as executions land. */
const PORTED_INTENTS = new Set(["spawn", "emoji", "quick_chat", "mark_disconnected", "toggle_pause"]);
/** Extra ported intent types for a work-in-progress branch (comma list in `OFSIM_PORTED_INTENTS`). */
for (const t of (process.env.OFSIM_PORTED_INTENTS ?? "").split(",")) if (t) PORTED_INTENTS.add(t);
/** Engine-created executions kept active (comma list in `OFSIM_PORTED_EXECS`: Nation, Tribe, DoomsdayClock). */
const PORTED_EXECS = new Set((process.env.OFSIM_PORTED_EXECS ?? "").split(",").filter((t) => t));

/** Classes made inert one by one (comma list in `OFSIM_INERT_CLASSES`) while the Rust side has only a stub of them. */
const INERT_CLASSES = new Set((process.env.OFSIM_INERT_CLASSES ?? "").split(",").filter((t) => t));
/** Executions created from inside ported code: init and tick do nothing. */
const INERT_EXECUTIONS = { AllianceExtensionExecution, AllianceRequestExecution, ConstructionExecution } as const;
/** Behaviour classes: each listed method does nothing and returns the given value. */
const INERT_BEHAVIOURS: Record<string, [{ prototype: object }, Record<string, unknown>]> = {
  AiAttackBehavior: [
    AiAttackBehavior,
    { maybeAttack: undefined, forceSendAttack: undefined, sendAttack: false, attackRandomTarget: undefined, findIncomingAttackPlayer: null, getNeighborTraitorToAttack: null, shouldAttack: false },
  ],
  NationStructureBehavior: [NationStructureBehavior, { handleStructures: false }],
  NationMIRVBehavior: [NationMIRVBehavior, { considerMIRV: false }],
  NationNukeBehavior: [NationNukeBehavior, { maybeSendNuke: undefined, findBestNukeTarget: null }],
};

export function applyInertPatch(): void {
  for (const [name, cls] of Object.entries(INERT_EXECUTIONS)) {
    if (!INERT_CLASSES.has(name)) continue;
    (cls.prototype as { init: () => void }).init = () => {};
    (cls.prototype as { tick: () => void }).tick = () => {};
  }
  for (const [name, [cls, methods]] of Object.entries(INERT_BEHAVIOURS)) {
    if (!INERT_CLASSES.has(name)) continue;
    for (const [method, value] of Object.entries(methods)) {
      (cls.prototype as Record<string, unknown>)[method] = () => value;
    }
  }
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
  for (const [name, cls] of inert) {
    if (PORTED_EXECS.has(name)) continue;
    (cls.prototype as { init: () => void }).init = () => {};
    (cls.prototype as { tick: () => void }).tick = () => {};
  }
}
