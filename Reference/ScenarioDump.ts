// SPDX-License-Identifier: AGPL-3.0-only
/**
 * Differential scenario: warms a record up for N ticks, then repeatedly mutates the territory with
 * a seeded LCG (conquer and relinquish through the player API) and ticks the game, writing the same
 * NDJSON trace as TraceDumper. `ofsim scenario` runs the identical program on the Rust engine.
 * It exercises territory bookkeeping, `PlayerExecution` cluster removal, conquests of whole
 * players and tile ownership without needing attack executions.
 *
 * Usage: tsx ofsim-reference/ScenarioDump.ts <record> <out.ndjson> <warmTicks> <rounds> <stepsPerRound> [focus] [freeOnly] [enclaves] [radius]
 * `focus` K > 0 mutates only the first K alive players; `freeOnly` 1 only takes unowned land.
 */
import fs from "fs";
import { createGameRunner } from "../src/core/GameRunner";
import { applyInertPatch } from "./InertPatch";
import { createMapLoader, loadRecord, resolveWorktree, silenceEngineLogs } from "./EngineSupport";

silenceEngineLogs();
const [recordPath, outPath, warmArg, roundsArg, stepsArg, focusArg = "0", freeOnlyArg = "0", enclavesArg = "0", radiusArg = "8"] = process.argv.slice(2);
const enclaves = Number(enclavesArg);
const radius = Number(radiusArg);
const focus = Number(focusArg);
const freeOnly = freeOnlyArg === "1";
const warm = Number(warmArg);
const rounds = Number(roundsArg);
const steps = Number(stepsArg);
applyInertPatch();
const { record, gameStart } = loadRecord(recordPath);
const runner = await createGameRunner(gameStart, undefined, createMapLoader(resolveWorktree(import.meta.url)), () => {});
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

let state = 12345;
function next(): number {
  state = (Math.imul(state, 1664525) + 1013904223) >>> 0;
  return state >>> 8;
}

for (let i = 0; i < warm; i++) tick();
// Enclave phase: the first alive player walls in the next `enclaves` players, then takes one of their tiles.
{
  const alive = game.players();
  const h = alive[0];
  for (let k = 1; k <= enclaves && k < alive.length; k++) {
    const t = alive[k];
    const spawn = t.spawnTile();
    if (spawn === undefined) continue;
    const cx = game.x(spawn);
    const cy = game.y(spawn);
    for (let dx = -radius; dx <= radius; dx++) {
      for (let dy = -(radius - Math.abs(dx)); dy <= radius - Math.abs(dx); dy++) {
        const x = cx + dx;
        const y = cy + dy;
        if (!game.isValidCoord(x, y)) continue;
        const tile = game.ref(x, y);
        if (game.isLand(tile) && !game.isImpassable(tile) && game.ownerID(tile) === 0) h.conquer(tile);
      }
    }
    const border = Array.from(t.borderTiles());
    if (border.length > 0) h.conquer(border[0]);
  }
  tick();
  for (let i = 0; i < 4; i++) tick();
}
for (let r = 0; r < rounds; r++) {
  for (let s = 0; s < steps; s++) {
    const alive = game.players();
    if (alive.length === 0) break;
    const p = alive[next() % (focus > 0 ? Math.min(focus, alive.length) : alive.length)];
    const border = Array.from(p.borderTiles());
    const b = border[next() % Math.max(1, border.length)];
    const ns = b === undefined ? [] : game.neighbors(b);
    const n = ns[next() % Math.max(1, ns.length)];
    if (n !== undefined && game.isLand(n) && !game.isImpassable(n) && game.ownerID(n) !== p.smallID() && (!freeOnly || game.ownerID(n) === 0)) p.conquer(n);
    if (next() % 40 === 0) {
      const tiles = Array.from(p.tiles());
      const t = tiles[next() % Math.max(1, tiles.length)];
      if (t !== undefined) p.relinquish(t);
    }
  }
  tick();
}
fs.closeSync(out);
