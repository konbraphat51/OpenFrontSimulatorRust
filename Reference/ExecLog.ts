// SPDX-License-Identifier: AGPL-3.0-only
//! Debug tool: replays a record on the upstream engine and logs every execution added from a tick range.
import { createRunnerFor, loadRecord, resolveWorktree, silenceEngineLogs } from "./EngineSupport";

async function main(): Promise<void> {
  silenceEngineLogs();
  const [recordPath, fromS, toS] = process.argv.slice(2);
  const from = Number(fromS), to = Number(toS);
  const worktree = resolveWorktree(import.meta.url);
  const { record, gameStart } = loadRecord(recordPath);
  const runner = await createRunnerFor(gameStart, worktree, () => {});
  const game = runner.game as any;
  const proto = Object.getPrototypeOf(game);
  const orig = proto.addExecution;
  proto.addExecution = function (...execs: any[]) {
    const t = this.ticks();
    if (t >= from && t <= to) {
      for (const e of execs) {
        const o: any = e;
        const owner = o._owner?.smallID?.() ?? o.player?.smallID?.() ?? o.owner?.smallID?.() ?? o._owner?.id?.() ?? "?";
        process.stdout.write(JSON.stringify({ tick: t, name: e.constructor.name, owner, troops: o.startTroops ?? o.troops, target: o._targetID ?? o.targetID ?? o.dst ?? o.tile ?? null }) + "\n");
      }
    }
    return orig.apply(this, execs);
  };
  for (const turn of record.turns) {
    const tick = game.ticks();
    if (tick > to) break;
    runner.addTurn(turn);
    runner.executeNextTick();
  }
}
main();
