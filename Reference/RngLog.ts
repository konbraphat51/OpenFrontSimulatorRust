// SPDX-License-Identifier: AGPL-3.0-only
//! Debug tool: replays a record on the upstream engine and logs every PseudoRandom draw (`<s3 before> <raw u32>`), optionally printing the stack of draw N.
//! Usage: tsx ofsim-reference/RngLog.ts <record> <out.txt> <until tick> [stackAt]
import fs from "node:fs";
import { PseudoRandom } from "@openfront/engine-lib/PseudoRandom";
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
  const orig = proto.next;
  proto.next = function (): number {
    const s3 = this.s3 | 0;
    const v = orig.call(this);
    fs.writeSync(out, `${s3} ${v * 4294967296}\n`);
    if (count++ === stackAt) {
      fs.writeSync(2, new Error().stack + "\n");
      process.exit(3);
    }
    return v;
  };
  const runner = await createRunnerFor(gameStart, worktree, () => {});
  const game = runner.game as any;
  for (const turn of record.turns) {
    if (game.ticks() > until) break;
    runner.addTurn(turn);
    runner.executeNextTick();
  }
  fs.closeSync(out);
}
main();
