// SPDX-License-Identifier: AGPL-3.0-only
/**
 * Replays a game record through the upstream engine and writes one NDJSON trace line per tick.
 *
 * Copied into an OpenFrontIO worktree (`<worktree>/ofsim-reference/`) and run with that worktree's
 * tsx. The trace format is the contract of `ofsim trace`; see Reference/README.md.
 *
 * Usage:
 *   tsx ofsim-reference/TraceDumper.ts <record.json[.gz]> <out.ndjson> [--full-at t1,t2,...] [--until tick] [--inert 1]
 */
import fs from "fs";
import { PlayerType } from "../src/core/game/Game";
import { createGameRunner } from "../src/core/GameRunner";
import {
  createRunnerFor,
  loadRecord,
  parseArguments,
  resolveWorktree,
  silenceEngineLogs,
} from "./EngineSupport";

/** Writes full-state dumps for the requested ticks to `<out>.full.ndjson`. */
function captureFull(runner: Awaited<ReturnType<typeof createGameRunner>>, tick: number): string {
  const game = runner.game;
  const map = game.map();
  // Builds before the packed tile updates have no tileStateBuffer: read the owners tile by tile.
  const hasBuffer = typeof (map as { tileStateBuffer?: unknown }).tileStateBuffer === "function";
  const state = hasBuffer ? map.tileStateBuffer() : new Uint16Array(0);
  const count = hasBuffer ? state.length : map.width() * map.height();
  const owners = new Array<number>(count);
  for (let i = 0; i < count; i++) owners[i] = hasBuffer ? state[i] & 0x0fff : game.ownerID(i);
  const units = game.units().map((u) => ({
    id: u.id(),
    type: u.type(),
    owner: u.owner().smallID(),
    tile: u.tile(),
    troops: u.troops(),
    level: u.level(),
    active: u.isActive(),
    hash: u.hash(),
  }));
  const players = game.allPlayers().map((p) => ({
    smallId: p.smallID(),
    type: p.type(),
    troops: p.troops(),
    gold: p.gold().toString(),
    tilesOwned: p.numTilesOwned(),
    isAlive: p.isAlive(),
    hasSpawned: p.hasSpawned(),
    hash: p.hash(),
  }));
  return JSON.stringify({ tick, full: { width: map.width(), height: map.height(), tileState: Array.from(state), owners, units, players } });
}

/** Replays the record and writes the trace; returns a short summary. */
async function main(): Promise<void> {
  silenceEngineLogs();
  const { positional, options } = parseArguments(process.argv.slice(2));
  if (positional.length < 2) {
    throw new Error("usage: TraceDumper.ts <record.json[.gz]> <out.ndjson> [--full-at t1,t2,...] [--until tick]");
  }
  const [recordPath, outPath] = positional;
  const fullAt = new Set((options.get("--full-at") ?? "").split(",").filter((s) => s !== "").map(Number));
  if (options.has("--inert")) (await import("./InertPatch")).applyInertPatch();
  const until = options.has("--until") ? Number(options.get("--until")) : Infinity;
  const worktree = resolveWorktree(import.meta.url);
  const { record, gameStart } = loadRecord(recordPath);

  let tickError: string | null = null;
  const runner = await createRunnerFor(gameStart, worktree, (update) => {
    if ("errMsg" in update) tickError = `${update.errMsg}\n${update.stack ?? ""}`;
  });
  const game = runner.game as unknown as { hash(): number } & typeof runner.game;
  const out = fs.openSync(outPath, "w");
  const fullOut = fullAt.size > 0 ? fs.openSync(`${outPath}.full.ndjson`, "w") : null;
  let checked = 0;
  let mismatches = 0;
  let firstMismatch = -1;
  let lastTick = -1;
  const started = performance.now();
  try {
    for (const turn of record.turns) {
      const tick = game.ticks();
      if (tick > until) break;
      runner.addTurn(turn);
      // Builds before v0.30 return void from executeNextTick and report failures through the callback only.
      if (runner.executeNextTick() === false || tickError !== null) throw new Error(tickError ?? `tick ${tick} failed`);
      // State is read after executeNextTick(), i.e. after the tick `tick` ran (ticks() is now tick + 1).
      const hash = game.hash();
      const recordHash = turn.hash ?? null;
      if (recordHash !== null) {
        checked++;
        if (recordHash !== hash) {
          mismatches++;
          if (firstMismatch < 0) firstMismatch = tick;
        }
      }
      const players = game.allPlayers().map((p) => {
        const units = p.units();
        let unitHashSum = 0;
        for (const u of units) unitHashSum += u.hash();
        return [p.smallID(), p.troops(), p.numTilesOwned(), p.gold().toString(), unitHashSum, p.isAlive(), units.length];
      });
      fs.writeSync(out, JSON.stringify({ tick, hash, recordHash, players }) + "\n");
      if (fullOut !== null && fullAt.has(tick)) fs.writeSync(fullOut, captureFull(runner, tick) + "\n");
      lastTick = tick;
    }
  } finally {
    fs.closeSync(out);
    if (fullOut !== null) fs.closeSync(fullOut);
  }
  const seconds = (performance.now() - started) / 1000;
  const humans = game.allPlayers().filter((p) => p.type() === PlayerType.Human).length;
  console.error(
    JSON.stringify({ gameID: record.info.gameID, ticks: lastTick + 1, humans, hashesChecked: checked, mismatches, firstMismatch, seconds, ticksPerSecond: (lastTick + 1) / seconds }),
  );
  process.exitCode = mismatches > 0 ? 2 : 0;
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
