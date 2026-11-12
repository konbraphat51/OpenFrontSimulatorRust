/**
 * Compares two NDJSON traces (TraceDumper or `ofsim trace`) and reports the first difference.
 *
 * Usage:
 *   tsx ofsim-reference/TraceDiff.ts a.ndjson b.ndjson
 * Exit code: 0 identical, 1 different, 2 usage error.
 */
import fs from "fs";
import readline from "readline";

const PLAYER_FIELDS = ["smallId", "troops", "tilesOwned", "gold", "unitHashSum", "isAlive", "numUnits"];

/** Streams the lines of a file. */
function lines(path: string): AsyncIterator<string> {
  return readline.createInterface({ input: fs.createReadStream(path), crlfDelay: Infinity })[Symbol.asyncIterator]();
}

/** Describes the first differing player/field of two parsed lines, or null when only top-level fields differ. */
function describePlayers(a: { players?: unknown[][] }, b: { players?: unknown[][] }): string | null {
  const pa = a.players ?? [];
  const pb = b.players ?? [];
  if (pa.length !== pb.length) return `players length ${pa.length} vs ${pb.length}`;
  for (let i = 0; i < pa.length; i++) {
    for (let f = 0; f < PLAYER_FIELDS.length; f++) {
      if (pa[i][f] !== pb[i][f]) {
        return `player index ${i} (smallId ${pa[i][0]} vs ${pb[i][0]}) field ${PLAYER_FIELDS[f]}: ${String(pa[i][f])} vs ${String(pb[i][f])}`;
      }
    }
  }
  return null;
}

async function main(): Promise<number> {
  const [pathA, pathB] = process.argv.slice(2);
  if (pathA === undefined || pathB === undefined) {
    console.error("usage: TraceDiff.ts a.ndjson b.ndjson");
    return 2;
  }
  const ia = lines(pathA);
  const ib = lines(pathB);
  let count = 0;
  for (;;) {
    const [ra, rb] = await Promise.all([ia.next(), ib.next()]);
    if (ra.done && rb.done) {
      console.log(`identical (${count} lines)`);
      return 0;
    }
    if (ra.done || rb.done) {
      console.log(`length differs: ${ra.done ? "a" : "b"} ended after ${count} lines`);
      return 1;
    }
    if (ra.value !== rb.value) {
      const a = JSON.parse(ra.value);
      const b = JSON.parse(rb.value);
      console.log(`first difference at line ${count + 1}, tick ${a.tick} vs ${b.tick}`);
      if (a.hash !== b.hash) console.log(`  hash: ${a.hash} vs ${b.hash}`);
      if (a.recordHash !== b.recordHash) console.log(`  recordHash: ${a.recordHash} vs ${b.recordHash}`);
      const detail = describePlayers(a, b);
      if (detail !== null) console.log(`  ${detail}`);
      return 1;
    }
    count++;
  }
}

main().then((code) => process.exit(code));
