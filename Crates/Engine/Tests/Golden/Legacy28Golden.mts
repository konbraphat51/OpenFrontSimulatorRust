// SPDX-License-Identifier: AGPL-3.0-only
// Generates Legacy28.json: the v0.28.4 pathfinding (MiniAStar, PathFinder.Mini, AirPathFinder, ParabolaPathFinder) on real maps.
// Run from the v0.28.4 worktree (commit 793efe4dd1ba, reads its resources/maps):
//   cd <worktree> && OPENFRONT_WORKTREE=<worktree> npx tsx <this dir>/Legacy28Golden.mts > <this dir>/Legacy28.json
// Per query it stores the inputs and a 32-bit FNV-1a hash of the result sequence.
import fs from "fs";
import path from "path";
import { upstream } from "./Common.mts";

const root = process.env.OPENFRONT_WORKTREE!;
const { GameMapSize, GameMapType } = await upstream("core/game/Game");
const { loadTerrainMap } = await upstream("core/game/TerrainMapLoader");
const { MiniAStar } = await upstream("core/pathfinding/MiniAStar");
const { PathFinder, AirPathFinder, ParabolaPathFinder } = await upstream("core/pathfinding/PathFinding");
const { PathFindResultType } = await upstream("core/pathfinding/AStar");
const { PseudoRandom } = await upstream("core/PseudoRandom");

function mulberry32(seed: number) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function fnv(values: ArrayLike<number>): number {
  let h = 0x811c9dc5;
  for (let i = 0; i < values.length; i++) {
    const v = values[i] >>> 0;
    for (let b = 0; b < 4; b++) {
      h ^= (v >>> (8 * b)) & 0xff;
      h = Math.imul(h, 16777619) >>> 0;
    }
  }
  return h >>> 0;
}

const mapsDir = path.join(root, "resources", "maps");
const loader = {
  getMapData(map: string) {
    const key = Object.keys(GameMapType).find((k) => GameMapType[k] === map)!;
    const dir = path.join(mapsDir, key.toLowerCase());
    const bin = (n: string) => async () => new Uint8Array(fs.readFileSync(path.join(dir, n)));
    return {
      mapBin: bin("map.bin"),
      map4xBin: bin("map4x.bin"),
      map16xBin: bin("map16x.bin"),
      manifest: async () => JSON.parse(fs.readFileSync(path.join(dir, "manifest.json"), "utf8")),
      webpPath: "",
    };
  },
};

const out: any = { maps: {} };
for (const mapName of ["World", "Black Sea", "Europe"]) {
  const t = await loadTerrainMap(mapName, GameMapSize.Normal, loader);
  const map = t.gameMap;
  const mini = t.miniGameMap;
  const rnd = mulberry32(12345 + mapName.length);
  const pick = (pred: (r: number) => boolean) => {
    for (;;) {
      const r = Math.floor(rnd() * map.width() * map.height());
      if (pred(r)) return r;
    }
  };
  const ocean = (r: number) => map.isOcean(r);
  const shore = (r: number) => map.isShore(r);
  const res: any = { width: map.width(), height: map.height(), mini: [], land: [], multi: [], stepper: [], air: [], parabola: [] };

  // MiniAStar on water: complete in one call and resumed across calls
  for (let i = 0; i < 24; i++) {
    const src = pick(ocean);
    const dst = pick(ocean);
    const [iterations, maxTries] = [[10000, 100], [300, 20], [2500, 20], [40, 5]][i % 4];
    const a = new MiniAStar(map, mini, src, dst, iterations, maxTries, true);
    const seq: number[] = [];
    let r;
    let calls = 0;
    do {
      r = a.compute();
      seq.push(r);
      calls++;
    } while (r === PathFindResultType.Pending && calls < 200);
    const p = r === PathFindResultType.Completed ? a.reconstructPath() : [];
    res.mini.push({ src, dst, iterations, maxTries, calls, last: r, len: p.length, hash: fnv(p), seq: fnv(seq) });
  }
  // MiniAStar on land with direction penalty (rail)
  for (let i = 0; i < 16; i++) {
    const src = pick((r) => map.isLand(r));
    const sx = map.x(src);
    const sy = map.y(src);
    let dst = src;
    for (let k = 0; k < 2000; k++) {
      const x = Math.max(0, Math.min(map.width() - 1, sx + Math.floor((rnd() - 0.5) * 120)));
      const y = Math.max(0, Math.min(map.height() - 1, sy + Math.floor((rnd() - 0.5) * 120)));
      const c = map.ref(x, y);
      if (map.isLand(c)) {
        dst = c;
        break;
      }
    }
    const a = new MiniAStar(map, mini, src, dst, 5000, 20, false, 3);
    let r;
    let calls = 0;
    do {
      r = a.compute();
      calls++;
    } while (r === PathFindResultType.Pending && calls < 50);
    const p = r === PathFindResultType.Completed ? a.reconstructPath() : [];
    res.land.push({ src, dst, calls, last: r, len: p.length, hash: fnv(p) });
  }
  // Multi-source (bestShoreDeploymentSource)
  for (let i = 0; i < 12; i++) {
    const srcs: number[] = [];
    for (let k = 0; k < 8; k++) srcs.push(pick(shore));
    const dst = pick(shore);
    const a = new MiniAStar(map, mini, srcs, dst, 1_000_000, 1);
    const r = a.compute();
    const p = r === PathFindResultType.Completed ? a.reconstructPath() : [];
    res.multi.push({ srcs, dst, last: r, len: p.length, hash: fnv(p) });
  }
  // PathFinder.Mini stepper
  const game: any = { map: () => map, miniMap: () => mini, manhattanDist: (a: number, b: number) => map.manhattanDist(a, b) };
  for (let i = 0; i < 10; i++) {
    const src = pick(ocean);
    const dst0 = pick(ocean);
    const dst1 = pick(ocean);
    let dst = dst0;
    const [iterations, maxTries, dist] = [[2500, 20, 1], [10000, 100, 5], [200, 20, 1]][i % 3];
    const pf = PathFinder.Mini(game, iterations, true, maxTries);
    let cur = src;
    const seq: number[] = [];
    let steps = 0;
    for (; steps < 6000; steps++) {
      if (steps === 400) dst = dst1; // the destination moves once
      const r = pf.nextTile(cur, dst, dist);
      seq.push(r.type, r.node ?? 0xffffffff);
      if (r.type === PathFindResultType.NextTile) cur = r.node;
      else if (r.type === PathFindResultType.Completed || r.type === PathFindResultType.PathNotFound) break;
    }
    res.stepper.push({ src, dst0, dst1, iterations, maxTries, dist, steps, hash: fnv(seq), end: cur });
  }
  // AirPathFinder
  for (let i = 0; i < 10; i++) {
    const src = pick(() => true);
    const dst = pick(() => true);
    const seed = 1000 + i * 37;
    const ap = new AirPathFinder(map, new PseudoRandom(seed));
    let cur = src;
    const seq: number[] = [];
    for (let s = 0; s < 4000; s++) {
      const r = ap.nextTile(cur, dst);
      if (r === true) {
        seq.push(0xfffffffe);
        break;
      }
      seq.push(r);
      cur = r;
    }
    res.air.push({ src, dst, seed, len: seq.length, hash: fnv(seq) });
  }
  // ParabolaPathFinder
  for (let i = 0; i < 10; i++) {
    const src = pick(() => true);
    const dst = pick(() => true);
    const increment = [3, 6, 8][i % 3];
    const based = i % 4 !== 3;
    const speed = [6, 8, 10, 1][i % 4];
    const pp = new ParabolaPathFinder(map);
    pp.computeControlPoints(src, dst, increment, based);
    const all = pp.allTiles();
    const seq: number[] = [];
    for (let s = 0; s < 3000; s++) {
      const r = pp.nextTile(speed);
      if (r === true) {
        seq.push(0xfffffffe);
        break;
      }
      seq.push(r, pp.currentIndex());
    }
    res.parabola.push({ src, dst, increment, based, speed, all: [all.length, fnv(all)], steps: seq.length, hash: fnv(seq) });
  }
  out.maps[mapName] = res;
}
console.log(JSON.stringify(out));
