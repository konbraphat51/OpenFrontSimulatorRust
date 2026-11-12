// SPDX-License-Identifier: AGPL-3.0-only
/**
 * Behaviour-level golden generator for the nation behaviours (NationStructureBehavior,
 * NationNukeBehavior, NationMIRVBehavior): builds a game from a spec, runs the scripted steps on
 * the upstream engine and writes one JSON line per step with the decisions the behaviours took.
 * `Crates/Engine/tests/NationBehaviorGoldenTest.rs` runs the identical spec on the Rust engine.
 *
 * Usage: tsx ofsim-reference/NationBehaviorDump.ts <spec.json> <out.ndjson>
 * The spec is `{ "gameID", "config": {...partial GameConfig}, "steps": [...] }`; see the Rust test for the step kinds.
 */
import fs from "fs";
import { createGameRunner } from "../src/core/GameRunner";
import { ConstructionExecution } from "../src/core/execution/ConstructionExecution";
import { MirvExecution } from "../src/core/execution/MIRVExecution";
import { NukeExecution } from "../src/core/execution/NukeExecution";
import { UpgradeStructureExecution } from "../src/core/execution/UpgradeStructureExecution";
import { NationMIRVBehavior } from "../src/core/execution/nation/NationMIRVBehavior";
import { NationNukeBehavior } from "../src/core/execution/nation/NationNukeBehavior";
import { NationStructureBehavior } from "../src/core/execution/nation/NationStructureBehavior";
import { PlayerInfo, PlayerType, UnitType } from "../src/core/game/Game";
import type { Game, Player, Unit } from "../src/core/game/Game";
import { PseudoRandom } from "../src/core/PseudoRandom";
import { createMapLoader, resolveWorktree, silenceEngineLogs } from "./EngineSupport";

silenceEngineLogs();
const [specPath, outPath] = process.argv.slice(2);
// eslint-disable-next-line @typescript-eslint/no-explicit-any
type Step = Record<string, any>;
const spec = JSON.parse(fs.readFileSync(specPath, "utf8")) as { gameID: string; config: Step; steps: Step[] };

const gameStart = {
  gameID: spec.gameID,
  lobbyCreatedAt: 0,
  config: {
    gameMap: "World",
    gameMapSize: "Normal",
    gameMode: "Free For All",
    gameType: "Singleplayer",
    difficulty: "Medium",
    nations: "disabled",
    donateGold: false,
    donateTroops: false,
    bots: 0,
    infiniteGold: false,
    infiniteTroops: false,
    instantBuild: false,
    randomSpawn: false,
    spawnImmunityDuration: 0,
    ...spec.config,
  },
  players: [],
};
// eslint-disable-next-line @typescript-eslint/no-explicit-any
const runner = await createGameRunner(gameStart as any, undefined, createMapLoader(resolveWorktree(import.meta.url)), () => {});
const game: Game = runner.game;
game.endSpawnPhase();

type Decision = [string, ...unknown[]];
let captured: Decision[] = [];
const originalAddExecution = game.addExecution.bind(game);
// eslint-disable-next-line @typescript-eslint/no-explicit-any
(game as any).addExecution = (...execs: any[]) => {
  for (const e of execs) {
    if (e instanceof ConstructionExecution) captured.push(["construct", e["constructionType"], e["tile"]]);
    else if (e instanceof UpgradeStructureExecution) captured.push(["upgrade", e["unitId"]]);
    else if (e instanceof NukeExecution) captured.push(["nuke", e["nukeType"], e["dst"], e["waitTicks"]]);
    else if (e instanceof MirvExecution) captured.push(["mirv", e["dst"]]);
    else originalAddExecution(e);
  }
};

const player = (id: string): Player => game.player(id);
const out = fs.openSync(outPath, "w");
const behaviors = new Map<string, unknown>();

/** Emoji double: no draws, no executions. */
const emojiDouble = { maybeSendAttackEmoji() {}, maybeSendEmoji() {}, sendEmoji() {} };

function applyDecisions(p: Player, decisions: Decision[]): void {
  for (const d of decisions) {
    if (d[0] === "construct") p.buildUnit(d[1] as UnitType, d[2] as number, {});
    else if (d[0] === "upgrade") p.upgradeUnit(game.unit(d[1] as number) as Unit);
  }
}

function digest(): number {
  return (game as unknown as { hash(): number }).hash();
}

let index = 0;
for (const step of spec.steps) {
  let ret: unknown = null;
  captured = [];
  switch (step.op) {
    case "addPlayer":
      game.addPlayer(new PlayerInfo(step.id, step.type as PlayerType, null, step.id), step.team ?? null);
      break;
    case "conquerSquare": {
      const p = player(step.player);
      for (let x = step.x - step.r; x <= step.x + step.r; x++) {
        for (let y = step.y - step.r; y <= step.y + step.r; y++) {
          if (!game.isValidCoord(x, y)) continue;
          const t = game.ref(x, y);
          if (game.isLand(t) && !game.isImpassable(t) && game.ownerID(t) === 0) p.conquer(t);
        }
      }
      break;
    }
    case "gold":
      player(step.player).addGold(BigInt(step.amount));
      break;
    case "troops":
      player(step.player).setTroops(step.amount);
      break;
    case "build": {
      const p = player(step.player);
      const params = step.targetX === undefined ? {} : { targetTile: game.ref(step.targetX, step.targetY) };
      const unit = p.buildUnit(step.type as UnitType, game.ref(step.x, step.y), params);
      for (let l = 1; l < (step.level ?? 1); l++) unit.increaseLevel();
      if (step.construction) unit.setUnderConstruction(true);
      ret = unit.id();
      break;
    }
    case "relation":
      player(step.a).updateRelation(player(step.b), step.delta);
      break;
    case "attack":
      player(step.attacker).createAttack(
        player(step.target),
        step.troops,
        step.x === undefined ? null : game.ref(step.x, step.y),
        new Set<number>(),
      );
      break;
    case "tick":
      for (let i = 0; i < step.n; i++) game.executeNextTick();
      break;
    case "structure.new":
      behaviors.set(step.id, new NationStructureBehavior(new PseudoRandom(step.seed), game, player(step.player)));
      break;
    case "structure.call": {
      const b = behaviors.get(step.id) as NationStructureBehavior;
      ret = b.handleStructures();
      if (step.apply) applyDecisions(player(step.player), captured);
      break;
    }
    case "nuke.new": {
      const attackDouble = {
        shouldAttack: () => step.shouldAttack ?? true,
        findIncomingAttackPlayer: () => (step.incoming ? player(step.incoming) : null),
      };
      behaviors.set(
        step.id,
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        new NationNukeBehavior(new PseudoRandom(step.seed), game, player(step.player), attackDouble as any, emojiDouble as any),
      );
      break;
    }
    case "nuke.call":
      (behaviors.get(step.id) as NationNukeBehavior).maybeSendNuke();
      break;
    case "nuke.target": {
      const target = (behaviors.get(step.id) as NationNukeBehavior).findBestNukeTarget();
      ret = target === null ? null : target.id();
      break;
    }
    case "mirv.new":
      behaviors.set(
        step.id,
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        new NationMIRVBehavior(new PseudoRandom(step.seed), game, player(step.player), emojiDouble as any),
      );
      break;
    case "mirv.call":
      ret = (behaviors.get(step.id) as NationMIRVBehavior).considerMIRV();
      break;
    default:
      throw new Error(`unknown step ${step.op}`);
  }
  fs.writeSync(out, JSON.stringify({ i: index, op: step.op, ret, decisions: captured, hash: digest() }) + "\n");
  index++;
}
fs.closeSync(out);
