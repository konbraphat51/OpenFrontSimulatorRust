// SPDX-License-Identifier: AGPL-3.0-only
/**
 * Debug patch for differential tests against `ofsim` built while executions are missing: makes
 * every execution the Rust engine has not ported inert in the upstream engine too (run
 * `ofsim` with `OFSIM_INERT=1` and the tracer with `--inert 1`). Traces made this way never match
 * the record hashes; they only compare the two engines with each other.
 */
import { AttackExecution } from "../src/core/execution/AttackExecution";
import { createRequire } from "node:module";
import { Executor } from "../src/core/execution/ExecutionManager";
import { MirvExecution } from "../src/core/execution/MIRVExecution";
import { NationExecution } from "../src/core/execution/NationExecution";
import { NoOpExecution } from "../src/core/execution/NoOpExecution";
import { ShellExecution } from "../src/core/execution/ShellExecution";
import { TribeExecution } from "../src/core/execution/TribeExecution";
import { AllianceExtensionExecution } from "../src/core/execution/alliance/AllianceExtensionExecution";
import { AllianceRequestExecution } from "../src/core/execution/alliance/AllianceRequestExecution";
import { ConstructionExecution } from "../src/core/execution/ConstructionExecution";
import { NationMIRVBehavior } from "../src/core/execution/nation/NationMIRVBehavior";
import { NationNukeBehavior } from "../src/core/execution/nation/NationNukeBehavior";
import { NationStructureBehavior } from "../src/core/execution/nation/NationStructureBehavior";
import { AiAttackBehavior } from "../src/core/execution/utils/AiAttackBehavior";
import { NukeExecution } from "../src/core/execution/NukeExecution";
import { SAMLauncherExecution } from "../src/core/execution/SAMLauncherExecution";
import { TradeShipExecution } from "../src/core/execution/TradeShipExecution";
import { WarshipExecution } from "../src/core/execution/WarshipExecution";

/** Intents whose executions are ported on the Rust side; extend as executions land. */
const PORTED_INTENTS = new Set(["spawn", "emoji", "quick_chat", "mark_disconnected", "toggle_pause"]);
/** Extra ported intent types for a work-in-progress branch (comma list in `OFSIM_PORTED_INTENTS`). */
for (const t of (process.env.OFSIM_PORTED_INTENTS ?? "").split(",")) if (t) PORTED_INTENTS.add(t);
/** Engine-created executions kept active (comma list in `OFSIM_PORTED_EXECS`: Nation, Tribe, DoomsdayClock). */
const PORTED_EXECS = new Set((process.env.OFSIM_PORTED_EXECS ?? "").split(",").filter((t) => t));

/** Classes made inert one by one (comma list in `OFSIM_INERT_CLASSES`; `Attack` also matches `AttackExecution`) while the Rust side has only a stub of them. */
const INERT_CLASSES = new Set(
  (process.env.OFSIM_INERT_CLASSES ?? "")
    .split(",")
    .filter((t) => t)
    .flatMap((t) => [t, `${t}Execution`]),
);
/** Executions created from inside ported code: init and tick do nothing. */
const INERT_EXECUTIONS = {
  AllianceExtensionExecution,
  AllianceRequestExecution,
  AttackExecution,
  ConstructionExecution,
  ShellExecution,
} as const;
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
/** Executions created from inside ported ones that are made inert (comma list in `OFSIM_INERT_EXECS`: Nuke, Mirv, SAMLauncher, TradeShip, Warship). */
const INERT_EXECS = new Set((process.env.OFSIM_INERT_EXECS ?? "").split(",").filter((t) => t));

/** The Doomsday Clock execution, or undefined in builds that predate it. */
function loadDoomsdayClockExecution(): (new (...args: never[]) => object) | undefined {
  try {
    const requireModule = createRequire(import.meta.url);
    return requireModule("../src/core/execution/DoomsdayClockExecution").DoomsdayClockExecution;
  } catch {
    return undefined;
  }
}

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
    ["DoomsdayClock", loadDoomsdayClockExecution()],
  ] as const;
  for (const [name, cls] of inert) {
    if (cls === undefined) continue; // builds without the Doomsday Clock (v0.32.6 and older)
    if (PORTED_EXECS.has(name)) continue;
    (cls.prototype as { init: () => void }).init = () => {};
    (cls.prototype as { tick: () => void }).tick = () => {};
  }
}
