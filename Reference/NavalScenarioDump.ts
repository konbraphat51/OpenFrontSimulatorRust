// SPDX-License-Identifier: AGPL-3.0-only
/**
 * Differential scenario for the water executions: warms a record up for N ticks, then builds one
 * port per player on its first shore tile, adds a warship, trade ships between neighbouring ports
 * and transport ships, retreats and moves some of them, and ticks the game while writing the same
 * NDJSON trace as TraceDumper. `ofsim naval-scenario` runs the identical program on the Rust engine.
 * Run with `OFSIM_INERT_CLASSES=Attack,Shell` (and `--inert` semantics of InertPatch) on both sides.
 *
 * Usage: tsx ofsim-reference/NavalScenarioDump.ts <record> <out.ndjson> <warmTicks> <ticks> [players]
 */
import fs from "fs";
import { UnitType } from "../src/core/game/Game";
import { BoatRetreatExecution } from "../src/core/execution/BoatRetreatExecution";
import { MoveWarshipExecution } from "../src/core/execution/MoveWarshipExecution";
import { TradeShipExecution } from "../src/core/execution/TradeShipExecution";
import { TransportShipExecution } from "../src/core/execution/TransportShipExecution";
import { WarshipExecution } from "../src/core/execution/WarshipExecution";
import { createGameRunner } from "../src/core/GameRunner";
import { applyInertPatch } from "./InertPatch";
import { createRunnerFor, loadRecord, resolveWorktree, silenceEngineLogs } from "./EngineSupport";

silenceEngineLogs();
const [recordPath, outPath, warmArg, ticksArg, playersArg = "6"] = process.argv.slice(2);
const warm = Number(warmArg);
const totalTicks = Number(ticksArg);
const maxPlayers = Number(playersArg);
applyInertPatch();
const { record, gameStart } = loadRecord(recordPath);
const runner = await createRunnerFor(gameStart, resolveWorktree(import.meta.url), () => {});
const game = runner.game;
const out = fs.openSync(outPath, "w");

function line(tick: number): void {
  const players = game.allPlayers().map((p) => {
    let unitHashSum = 0;
    for (const u of p.units()) unitHashSum += u.hash();
    return [p.smallID(), p.troops(), p.numTilesOwned(), p.gold().toString(), unitHashSum, p.isAlive(), p.units().length];
  });
  const hash = (game as unknown as { hash(): number }).hash();
  fs.writeSync(out, JSON.stringify({ tick, hash, recordHash: null, players }) + "\n");
}

let nextTurn = 0;
function tick(): void {
  const t = game.ticks();
  const turn = nextTurn < warm ? record.turns[nextTurn] : { turnNumber: nextTurn, intents: [] };
  nextTurn++;
  runner.addTurn(turn);
  if (!runner.executeNextTick()) throw new Error(`tick ${t} failed`);
  line(t);
}

for (let i = 0; i < warm; i++) tick();

const shoreOf = (p: ReturnType<typeof game.players>[number]) => {
  for (const t of p.borderTiles()) if (game.isShore(t) && game.isLand(t) && game.ownerID(t) === p.smallID()) return t;
  return undefined;
};
// The first players (in id order) that own a shore tile
const cast = game.players().filter((p) => shoreOf(p) !== undefined).slice(0, maxPlayers);
const ports: { owner: (typeof cast)[number]; unit: ReturnType<(typeof cast)[number]["buildUnit"]> }[] = [];
const warships: number[] = [];
for (const p of cast) {
  p.addGold(100_000_000n);
  const shore = shoreOf(p);
  if (shore === undefined) continue;
  const unit = p.buildUnit(UnitType.Port, shore, {});
  ports.push({ owner: p, unit });
  const water = game.neighbors(shore).find((n) => game.isWater(n));
  if (water !== undefined) game.addExecution(new WarshipExecution({ owner: p, patrolTile: water }));
}

function sendTrade(offset: number): void {
  for (let i = 0; i < ports.length; i++) {
    const a = ports[i];
    const b = ports[(i + 1 + offset) % ports.length];
    if (a === b || !game.hasWaterComponent(a.unit.tile(), game.getWaterComponent(b.unit.tile()) ?? -1)) continue;
    game.addExecution(new TradeShipExecution(a.owner, a.unit, b.unit));
  }
}

function sendBoats(): void {
  for (let i = 0; i < cast.length; i++) {
    const other = cast[(i + 1) % cast.length];
    const target = shoreOf(other);
    if (target !== undefined) game.addExecution(new TransportShipExecution(cast[i], target, null));
  }
}

for (let s = 0; s < totalTicks; s++) {
  if (s === 0) {
    sendTrade(0);
    sendBoats();
  }
  if (s === 1) {
    for (const u of game.units(UnitType.Warship)) warships.push(u.id());
  }
  if (s === 60) {
    // Retreat the first transport ship of every player
    for (const p of cast) {
      const boat = p.units(UnitType.TransportShip)[0];
      if (boat !== undefined && boat.id() % 2 === 0) game.addExecution(new BoatRetreatExecution(p, boat.id()));
    }
  }
  if (s === 150) {
    // Move every warship 30 tiles along x when that tile is water
    for (const p of cast) {
      for (const w of p.units(UnitType.Warship)) {
        const x = game.x(w.tile()) + 30;
        const y = game.y(w.tile());
        if (game.isValidCoord(x, y) && game.isWater(game.ref(x, y))) game.addExecution(new MoveWarshipExecution(p, [w.id(), w.id()], game.ref(x, y)));
      }
    }
  }
  if (s === 300 && ports.length > 1) {
    // Turn the second player's port tile into water: boats heading there must retreat
    const tile = ports[1].unit.tile();
    ports[1].owner.relinquish(tile);
    game.setWater(tile);
  }
  if (s > 0 && s % 250 === 0) {
    sendTrade((s / 250) % 3);
    sendBoats();
  }
  tick();
}
fs.closeSync(out);
