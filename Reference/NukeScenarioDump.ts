// SPDX-License-Identifier: AGPL-3.0-only
/**
 * Differential scenario for nukes, SAMs and the Doomsday Clock: warms a record up, gives players
 * structures and gold through the player API, then launches seeded atom bombs, hydrogen bombs and
 * MIRVs by adding the executions directly. `ofsim nuke-scenario` runs the identical program on the
 * Rust engine. Writes the usual NDJSON trace (every `OFSIM_TRACE_EVERY` ticks) plus `<out>.digest` (per tick: fallout, land,
 * a hash of the packed tile state and the game hash) so fallout and water conversion are compared too.
 *
 * Usage: tsx ofsim-reference/NukeScenarioDump.ts <record> <out.ndjson> <warm> <rounds> <spacing> <tail> [doomsdaySpeed|none] [attackers] [grow] [samMod]
 * Run with OFSIM_PORTED_EXECS=DoomsdayClock (and --inert behaviour via InertPatch) like the other scenarios.
 */
import fs from "fs";
import { DoomsdayClockExecution } from "../src/core/execution/DoomsdayClockExecution";
import { MirvExecution } from "../src/core/execution/MIRVExecution";
import { NukeExecution } from "../src/core/execution/NukeExecution";
import { SAMLauncherExecution } from "../src/core/execution/SAMLauncherExecution";
import { PlayerType, UnitType } from "../src/core/game/Game";
import { createGameRunner } from "../src/core/GameRunner";
import { applyInertPatch } from "./InertPatch";
import { createMapLoader, loadRecord, resolveWorktree, silenceEngineLogs } from "./EngineSupport";

void DoomsdayClockExecution;
silenceEngineLogs();
const [recordPath, outPath, warmArg, roundsArg, spacingArg, tailArg, doomsdayArg = "none", attackersArg = "4", growArg = "25", samModArg = "25"] = process.argv.slice(2);
const warm = Number(warmArg);
const rounds = Number(roundsArg);
const spacing = Number(spacingArg);
const tail = Number(tailArg);
const attackerCount = Number(attackersArg);
const grow = Number(growArg);
const samMod = Number(samModArg);
applyInertPatch();
const { record, gameStart } = loadRecord(recordPath);
if (doomsdayArg !== "none") {
  (gameStart.config as { doomsdayClock?: unknown }).doomsdayClock = { enabled: true, speed: doomsdayArg };
}
const runner = await createGameRunner(gameStart, undefined, createMapLoader(resolveWorktree(import.meta.url)), () => {});
const game = runner.game;
const out = fs.openSync(outPath, "w");
const digest = fs.openSync(`${outPath}.digest`, "w");

const traceEvery = Number(process.env.OFSIM_TRACE_EVERY ?? 1);

function line(tick: number): void {
  const hash = (game as unknown as { hash(): number }).hash();
  if (tick % traceEvery === 0) writeTrace(tick, hash);
  const state = game.map().tileStateBuffer();
  let h = 2166136261;
  for (let i = 0; i < state.length; i++) h = Math.imul(h ^ state[i], 16777619) >>> 0;
  fs.writeSync(digest, `${tick},${game.numTilesWithFallout()},${game.numLandTiles()},${h},${hash}\n`);
}

function writeTrace(tick: number, hash: number): void {
  const players = game.allPlayers().map((p) => {
    let unitHashSum = 0;
    for (const u of p.units()) unitHashSum += u.hash();
    return [p.smallID(), p.troops(), p.numTilesOwned(), p.gold().toString(), unitHashSum, p.isAlive(), p.units().length];
  });
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

let state = 987654321;
function next(): number {
  state = (Math.imul(state, 1664525) + 1013904223) >>> 0;
  return state >>> 8;
}

for (let i = 0; i < warm; i++) tick();

const alive = game.players();
// Grow phase: every alive player takes the free land within `grow` Manhattan steps of its spawn.
for (const p of alive) {
  const spawn = p.spawnTile();
  if (spawn === undefined) continue;
  const cx = game.x(spawn);
  const cy = game.y(spawn);
  for (let dx = -grow; dx <= grow; dx++) {
    for (let dy = -(grow - Math.abs(dx)); dy <= grow - Math.abs(dx); dy++) {
      const x = cx + dx;
      const y = cy + dy;
      if (!game.isValidCoord(x, y)) continue;
      const tile = game.ref(x, y);
      if (game.isLand(tile) && !game.isImpassable(tile) && game.ownerID(tile) === 0) p.conquer(tile);
    }
  }
}
const humans = alive.filter((p) => p.type() === PlayerType.Human);
const attackers = (humans.length > 0 ? humans : alive).slice(0, attackerCount);
alive.forEach((p, index) => {
  p.addGold(10n ** 13n);
  const tiles = Array.from(p.tiles());
  if (tiles.length === 0) return;
  const pick = () => tiles[next() % tiles.length];
  if (samMod > 0 && index % samMod === 0) {
    const sam = p.buildUnit(UnitType.SAMLauncher, pick(), {});
    game.addExecution(new SAMLauncherExecution(p, null, sam));
    if (index % (2 * samMod) === 0) p.upgradeUnit(sam);
  }
  p.buildUnit(UnitType.City, pick(), {});
  p.buildUnit(UnitType.DefensePost, pick(), {});
  if (attackers.includes(p)) {
    for (let i = 0; i < 3; i++) p.buildUnit(UnitType.MissileSilo, pick(), {});
  }
});
// Diplomacy: some accepted alliances and some pending requests towards attackers.
for (let k = 0; k < 10; k++) {
  const a = attackers[next() % attackers.length];
  const b = alive[next() % alive.length];
  if (a === b) continue;
  if (k < 6) {
    const request = a.createAllianceRequest(b);
    if (request !== null) request.accept();
  } else {
    b.createAllianceRequest(a);
  }
}
tick();

for (let r = 0; r < rounds; r++) {
  if (r === 10) for (const p of alive) for (const sam of p.units(UnitType.SAMLauncher)) p.upgradeUnit(sam);
  const ai = next();
  const ti = next();
  const ki = next();
  const tilei = next();
  const a = attackers[ai % attackers.length];
  const t = alive[ti % alive.length];
  const tiles = Array.from(t.tiles());
  if (a !== t && tiles.length > 0 && a.isAlive()) {
    const tile = tiles[tilei % tiles.length];
    a.addGold(10n ** 12n);
    const k = ki % 6;
    if (k < 3) game.addExecution(new NukeExecution(UnitType.AtomBomb, a, tile, null));
    else if (k < 5) game.addExecution(new NukeExecution(UnitType.HydrogenBomb, a, tile, null));
    else game.addExecution(new MirvExecution(a, tile));
  }
  for (let s = 0; s < spacing; s++) tick();
}
for (let i = 0; i < tail; i++) tick();
fs.closeSync(out);
fs.closeSync(digest);
