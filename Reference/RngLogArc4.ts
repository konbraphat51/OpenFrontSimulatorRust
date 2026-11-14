// SPDX-License-Identifier: AGPL-3.0-only
//! Debug tool: replays a record on an era S upstream engine (seedrandom PseudoRandom) and logs every draw as `A <first value> <draw index> <value>`.
//! Usage: tsx ofsim-reference/RngLogArc4.ts <record> <out.txt> <until tick> [stackAt]
import fs from "node:fs";
import { PseudoRandom } from "../src/core/PseudoRandom";
import { createRunnerFor, loadRecord, resolveWorktree, silenceEngineLogs } from "./EngineSupport";

async function main(): Promise<void> {
  silenceEngineLogs();
  const [recordPath, outPath, untilS, stackAtS] = process.argv.slice(2);
  const until = Number(untilS);
  const stackAt = stackAtS === undefined ? -1 : Number(stackAtS);
  const worktree = resolveWorktree(import.meta.url);
  const { record, gameStart } = loadRecord(recordPath);
  const out = fs.openSync(outPath, "w");
  let count = 0;
  const proto = PseudoRandom.prototype as any;
  // Each instance's `rng` is wrapped on first use; the stream key is its first drawn value.
  for (const name of Object.getOwnPropertyNames(proto)) {
    const fn = proto[name];
    if (name === "constructor" || typeof fn !== "function") continue;
    proto[name] = function (this: any, ...args: unknown[]) {
      if (!this.__wrapped) {
        this.__wrapped = true;
        const raw = this.rng;
        let first: number | null = null;
        let n = 0;
        this.rng = () => {
          const v = raw();
          if (first === null) first = v;
          fs.writeSync(out, `A ${first} ${n++} ${v}\n`);
          if (count++ === stackAt) {
            fs.writeSync(2, new Error().stack + "\n");
            process.exit(3);
          }
          return v;
        };
      }
      return fn.apply(this, args);
    };
  }
  const runner = await createRunnerFor(gameStart, worktree, () => {});
  const game = runner.game as any;
  for (const turn of record.turns) {
    if (game.ticks() > until) break;
    runner.addTurn(turn);
    fs.writeSync(out, `T ${game.ticks()}\n`);
    runner.executeNextTick();
  }
  fs.closeSync(out);
}
main();
