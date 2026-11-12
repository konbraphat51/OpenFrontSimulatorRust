// SPDX-License-Identifier: AGPL-3.0-only
// Generates Pathfinding.json: upstream pathfinding, water graph and WaterManager results on real maps.
// Run from the baseline worktree (it reads resources/maps there):
//   cd <worktree> && OPENFRONT_WORKTREE=<worktree> npx tsx <this dir>/PathfindingGolden.mts > <this dir>/Pathfinding.json
// Per query it stores the inputs, the path length and a 32-bit FNV-1a hash of the tile sequence (little-endian
// bytes); a few queries keep the full path. Timings of transport-ship queries go to stderr.
import fs from "fs";
import path from "path";
import { bits, upstream } from "./Common.mts";

const root = process.env.OPENFRONT_WORKTREE!;
const { GameMapSize, GameMapType } = await upstream("core/game/Game");
const { loadTerrainMap } = await upstream("core/game/TerrainMapLoader");
const { GameMapImpl } = await upstream("core/game/GameMap");
const { WaterManager } = await upstream("core/game/WaterManager");
const { PathFinding, UniversalPathFinding, WaterPathFinder } = await upstream("core/pathfinding/PathFinder");
const { PathStatus } = await upstream("core/pathfinding/types");
const { SpatialQuery } = await upstream("core/pathfinding/spatial/SpatialQuery");
const { AStarWater } = await upstream("core/pathfinding/algorithms/AStar.Water");
const { AStarWaterBounded } = await upstream("core/pathfinding/algorithms/AStar.WaterBounded");
const { ConnectedComponents } = await upstream("core/pathfinding/algorithms/ConnectedComponents");
const { getParabolaControlPoints } = await upstream("core/pathfinding/PathFinder.Parabola");
const { DistanceBasedBezierCurve } = await upstream("core/utilities/Line");

// ---- helpers -------------------------------------------------------------------------------------------------

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

/** FNV-1a over the little-endian bytes of each number. */
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

/** Keeps a full path for a few short queries only (per kind), to bound the golden's size. */
const fullBudget: Record<string, number> = {};
function keep(kind: string, p: number[] | null, budget = 2, maxLen = 700): number[] | undefined {
  if (!(kind in fullBudget)) fullBudget[kind] = budget;
  if (p === null || p.length > maxLen || fullBudget[kind] <= 0) return undefined;
  fullBudget[kind]--;
  return p;
}

/** [length, hash] of a path, or [-1, 0] for null. */
function digest(p: ArrayLike<number> | null): [number, number] {
  return p === null ? [-1, 0] : [p.length, fnv(p)];
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
      layerPng: async () => {
        throw new Error("no layers");
      },
    };
  },
};

function cloneMap(m: any) {
  const gm = new GameMapImpl(m.width(), m.height(), Uint8Array.from(m.terrain), m.numLandTiles());
  return gm;
}

/** A minimal Game with the members that pathfinding reads. */
function mockGame(map: any, mini: any, wm: any, opts: { noHpa?: boolean; ticks?: number } = {}) {
  return {
    map: () => map,
    miniMap: () => mini,
    miniWaterHPA: () => (opts.noHpa ? null : wm.miniWaterHPA()),
    miniWaterGraph: () => (opts.noHpa ? null : wm.miniWaterGraph()),
    waterGraphVersion: () => wm.waterGraphVersion(),
    isValidRef: (t: number) => map.isValidRef(t),
    manhattanDist: (a: number, b: number) => map.manhattanDist(a, b),
    ticks: () => opts.ticks ?? 0,
    x: (t: number) => map.x(t),
    y: (t: number) => map.y(t),
    ref: (x: number, y: number) => map.ref(x, y),
    width: () => map.width(),
    height: () => map.height(),
    isShore: (t: number) => map.isShore(t),
    isLand: (t: number) => map.isLand(t),
    isWater: (t: number) => map.isWater(t),
    ownerID: (t: number) => map.ownerID(t),
    getWaterComponent: (t: number) => wm.getWaterComponent(t),
  } as any;
}

function graphDigest(graph: any) {
  const nodes = graph.getAllNodes();
  const edges = graph.getAllEdges();
  const nodeVals: number[] = [];
  for (const n of nodes) nodeVals.push(n.id, n.x, n.y, n.tile, n.componentId);
  const edgeVals: number[] = [];
  for (const e of edges) edgeVals.push(e.id, e.nodeA, e.nodeB, e.cost, e.clusterX, e.clusterY);
  const clusterVals: number[] = [];
  for (let cy = 0; cy < graph.clustersY; cy++)
    for (let cx = 0; cx < graph.clustersX; cx++) {
      const c = graph.getCluster(cx, cy);
      clusterVals.push(c.nodeIds.length, ...c.nodeIds);
    }
  const adj: number[] = [];
  for (const n of nodes) {
    const es = graph.getNodeEdges(n.id);
    adj.push(es.length, ...es.map((e: any) => e.id));
  }
  return {
    nodeCount: graph.nodeCount,
    edgeCount: graph.edgeCount,
    nodesHash: fnv(nodeVals),
    edgesHash: fnv(edgeVals),
    clustersHash: fnv(clusterVals),
    adjacencyHash: fnv(adj),
  };
}

// ---- per-map generation ----------------------------------------------------------------------------------------

async function generate(name: string, sizeName: string, seed: number) {
  for (const k of Object.keys(fullBudget)) delete fullBudget[k];
  const rnd = mulberry32(seed);
  const rint = (n: number) => Math.floor(rnd() * n);
  const size = sizeName === "Compact" ? GameMapSize.Compact : GameMapSize.Normal;
  const data = await loadTerrainMap(name, size, loader, false);
  const map: any = data.gameMap;
  const mini: any = data.miniGameMap;
  const w = map.width(), h = map.height(), mw = mini.width(), mh = mini.height();
  const N = w * h, MN = mw * mh;

  const pick = (pred: (t: number) => boolean, total = N): number => {
    for (;;) {
      const t = rint(total);
      if (pred(t)) return t;
    }
  };
  const water = () => pick((t) => map.isWater(t));
  const shore = () => pick((t) => map.isShore(t));
  const miniWater = () => pick((t) => mini.isWater(t), MN);
  const miniShoreLand = () => pick((t) => mini.isLand(t), MN);

  const out: any = { name, size: sizeName, width: w, height: h, miniWidth: mw, miniHeight: mh, seed };

  // Water manager (graph, HPA) on the pristine maps
  const wm = new WaterManager(map, mini, false);
  const graph = wm.miniWaterGraph();
  out.graph = graphDigest(graph);
  const miniTiles = Array.from({ length: 300 }, () => rint(MN));
  out.components = miniTiles.map((t) => [t, graph.getComponentId(t)]);
  const compIds = new Set(out.components.map((c: number[]) => c[1]));
  out.componentSizes = [...compIds].filter((c: any) => c > 0 && c < 60000).slice(0, 30).map((c: any) => [c, graph.getComponentSize(c)]);
  const cc = new ConnectedComponents(mini);
  cc.initialize();
  out.componentsDirect = miniTiles.slice(0, 100).map((t) => [t, cc.getComponentId(t)]);
  const waterQ = Array.from({ length: 120 }, () => rint(N));
  out.waterComponent = waterQ.map((t) => [t, wm.getWaterComponent(t)]);
  out.hasWaterComponent = waterQ.slice(0, 60).map((t, i) => {
    const comp = wm.getWaterComponent(t);
    const probe = i % 2 === 0 && comp !== null ? comp : rint(6);
    return [t, probe, wm.hasWaterComponent(t, probe)];
  });
  out.waterComponentSize = waterQ.slice(0, 60).map((t) => [t, wm.getWaterComponentSize(t)]);

  // Whole-mini-map water A*
  {
    const aw = new AStarWater(mini);
    out.astarWater = [];
    for (let i = 0; i < 60; i++) {
      const from = miniWater();
      const to = i % 3 === 0 ? miniShoreLand() : miniWater();
      const starts = i % 4 === 3 ? [from, miniWater(), miniWater()] : from;
      const p = aw.findPath(starts, to);
      out.astarWater.push({ from: starts, to, d: digest(p), full: keep('astar', p) });
    }
  }

  // Bounded water A*
  {
    const ab = new AStarWaterBounded(mini, 100 * 100);
    out.bounded = [];
    for (let i = 0; i < 80; i++) {
      const from = miniWater();
      const fx = mini.x(from), fy = mini.y(from);
      const tx = Math.max(0, Math.min(mw - 1, fx + rint(61) - 30));
      const ty = Math.max(0, Math.min(mh - 1, fy + rint(61) - 30));
      const to = mini.ref(tx, ty);
      const pad = rint(12);
      const bounds = {
        minX: Math.max(0, Math.min(fx, tx) - pad),
        maxX: Math.min(mw - 1, Math.max(fx, tx) + pad),
        minY: Math.max(0, Math.min(fy, ty) - pad),
        maxY: Math.min(mh - 1, Math.max(fy, ty) + pad),
      };
      const multi = i % 5 === 4;
      const starts = multi ? [from, miniWater(), from + 1 < MN ? from + 1 : from] : from;
      const p = multi ? ab.searchBounded(starts, to, bounds) : ab.searchBounded(from, to, bounds);
      const p2 = i % 7 === 0 ? ab.findPath(starts, to) : null;
      out.bounded.push({ from: starts, to, bounds, d: digest(p), d2: i % 7 === 0 ? digest(p2) : undefined });
    }
  }

  // HPA* directly on the mini map (single and multi source)
  {
    const hpa = wm.miniWaterHPA();
    out.hpaSingle = [];
    for (let i = 0; i < 150; i++) {
      const from = miniWater();
      let to: number;
      if (i % 3 === 0) {
        const fx = mini.x(from), fy = mini.y(from);
        to = mini.ref(Math.max(0, Math.min(mw - 1, fx + rint(81) - 40)), Math.max(0, Math.min(mh - 1, fy + rint(81) - 40)));
      } else to = miniWater();
      const p = hpa.findPath(from, to);
      out.hpaSingle.push({ from, to, d: digest(p), full: keep('hpa', p) });
    }
    out.hpaMulti = [];
    for (let i = 0; i < 100; i++) {
      const to = miniWater();
      const tx = mini.x(to), ty = mini.y(to);
      const near = i % 2 === 0;
      const k = 2 + rint(5);
      const from: number[] = [];
      for (let j = 0; j < k; j++) {
        if (near) from.push(mini.ref(Math.max(0, Math.min(mw - 1, tx + rint(301) - 150)), Math.max(0, Math.min(mh - 1, ty + rint(301) - 150))));
        else from.push(miniWater());
      }
      const p = hpa.findPath(from, to);
      out.hpaMulti.push({ from, to, d: digest(p) });
    }
  }

  // Transport / trade / warship paths through the shared chain
  const game = mockGame(map, mini, wm);
  const gameSimple = mockGame(map, mini, wm, { noHpa: true });
  const timing: number[] = [];
  const pairs = (n: number) => {
    const list: number[][] = [];
    for (let i = 0; i < n; i++) {
      const kind = i % 4;
      const a = kind === 3 ? water() : shore();
      let b = kind === 2 ? water() : kind === 3 ? water() : shore();
      if (i % 5 === 0) {
        // nearby target
        const ax = map.x(a), ay = map.y(a);
        const t = map.ref(Math.max(0, Math.min(w - 1, ax + rint(161) - 80)), Math.max(0, Math.min(h - 1, ay + rint(161) - 80)));
        if (map.isWater(t) || map.isShore(t)) b = t;
      }
      list.push([a, b]);
    }
    return list;
  };
  out.chain = [];
  const chainPairs = pairs(300);
  for (let i = 0; i < chainPairs.length; i++) {
    const [a, b] = chainPairs[i];
    const t0 = performance.now();
    const p = PathFinding.Water(game).findPath(a, b);
    if (i % 4 !== 3 && i % 4 !== 2) timing.push(performance.now() - t0);
    out.chain.push({ from: a, to: b, d: digest(p), full: keep('chain', p, 6) });
  }
  out.chainMulti = [];
  for (let i = 0; i < 80; i++) {
    const [a, b] = chainPairs[i];
    const k = 1 + rint(5);
    const from = [a];
    for (let j = 1; j < k; j++) from.push(i % 2 === 0 ? shore() : water());
    const p = PathFinding.Water(game).findPath(from, b);
    out.chainMulti.push({ from, to: b, d: digest(p), full: keep('chainMulti', p) });
  }
  out.chainSimple = [];
  for (let i = 0; i < 40; i++) {
    const [a, b] = chainPairs[i];
    const p = PathFinding.Water(gameSimple).findPath(a, b);
    out.chainSimple.push({ from: a, to: b, d: digest(p), full: keep('simple', p) });
  }
  out.chainSimpleMulti = [];
  for (let i = 0; i < 20; i++) {
    const [a, b] = chainPairs[i];
    const from = [a, shore(), shore()];
    const p = PathFinding.Water(gameSimple).findPath(from, b);
    out.chainSimpleMulti.push({ from, to: b, d: digest(p) });
  }
  // Memoised path finder gives the same answers (checked here, not stored)
  {
    const wpf = new WaterPathFinder(game, 0, true);
    const plain = new WaterPathFinder(game, 0, false);
    for (let i = 0; i < 20; i++) {
      const [a, b] = chainPairs[i];
      const x = wpf.findPath(a, b), y = plain.findPath(a, b), x2 = wpf.findPath(a, b);
      if (JSON.stringify(x) !== JSON.stringify(y) || JSON.stringify(x2) !== JSON.stringify(y)) throw new Error("memo differs");
    }
  }

  // Stepping walks (WaterPathFinder.next)
  out.walks = [];
  for (let i = 0; i < 40; i++) {
    const [a, b] = chainPairs[i];
    const dist = i % 3 === 1 ? 5 : undefined;
    const wpf = new WaterPathFinder(game, 0, i % 2 === 0);
    let cur = a;
    const seq: number[] = [];
    const statuses: number[] = [];
    let trav: [number, number] | null = null;
    for (let s = 0; s < 200000; s++) {
      const r = wpf.next(cur, b, dist);
      statuses.push(r.status);
      if (r.status === PathStatus.NEXT) {
        if (s === 0) {
          const tp = wpf.pathForTraversal(cur, b);
          trav = digest(tp as any);
        }
        cur = r.node;
        seq.push(cur);
      } else {
        if (r.status === PathStatus.COMPLETE) seq.push(r.node);
        break;
      }
    }
    out.walks.push({ from: a, to: b, dist, d: digest(seq), last: statuses[statuses.length - 1], trav });
  }
  const meanMs = timing.reduce((x, y) => x + y, 0) / Math.max(1, timing.length);
  console.error(`${name} ${sizeName}: transport/trade chain query mean ${meanMs.toFixed(3)} ms over ${timing.length}`);

  // Spatial queries with a blocky ownership pattern on land (0 = unowned)
  {
    const ownerOf = (x: number, y: number) => ((x >> 6) + 2 * (y >> 6)) % 5;
    for (let t = 0; t < N; t++) if (map.isLand(t) && !map.isImpassable(t)) map.setOwnerID(t, ownerOf(t % w, Math.floor(t / w)));
    const borders = new Map<number, Set<number>>();
    for (let p = 0; p <= 4; p++) borders.set(p, new Set());
    for (let t = 0; t < N; t++) {
      const o = map.ownerID(t);
      if (o !== 0 && map.isBorder(t)) borders.get(o)!.add(t);
    }
    const owner = (p: number) => ({
      smallID: () => p,
      isPlayer: () => p !== 0,
      borderTiles: () => borders.get(p)!,
      tileChangeVersion: () => 1,
    });
    const sq = new SpatialQuery(game);
    const near = () => {
      const s = shore();
      return map.ref(Math.max(0, Math.min(w - 1, map.x(s) + rint(61) - 30)), Math.max(0, Math.min(h - 1, map.y(s) + rint(61) - 30)));
    };
    out.ownerPattern = "((x >> 6) + 2 * (y >> 6)) % 5 on land that is not impassable";
    out.closestShore = [];
    for (let i = 0; i < 80; i++) {
      const o = i % 5, t = near(), md = [undefined, 10, 100, 0][i % 4];
      const r = sq.closestShore(owner(o), t, md);
      out.closestShore.push([o, t, md ?? null, r ?? -1]);
    }
    out.closestReachableShore = [];
    for (let i = 0; i < 80; i++) {
      const target = i % 5, attacker = 1 + (i % 4), t = near(), md = [undefined, 10, 100, 0][i % 4];
      const r = sq.closestReachableShore(owner(target), owner(attacker), t, md);
      out.closestReachableShore.push([target, attacker, t, md ?? null, r ?? -1]);
    }
    out.closestShoreByWater = [];
    for (let i = 0; i < 120; i++) {
      const o = i % 5;
      const target = i % 2 === 0 ? water() : shore();
      const r = sq.closestShoreByWater(owner(o), target);
      out.closestShoreByWater.push([o, target, r ?? -1]);
    }
  }

  // Parabola arcs
  {
    const variants = [
      undefined,
      { increment: 1 },
      { increment: 5, distanceBasedHeight: false },
      { directionUp: false },
      { increment: 2.5, directionUp: false, distanceBasedHeight: false },
      { ignoreMapBounds: true },
    ];
    out.parabola = [];
    for (let i = 0; i < 90; i++) {
      const opt = variants[i % variants.length];
      let a = rint(N), b = rint(N);
      if (i % 6 === 1) b = map.ref(Math.max(0, Math.min(w - 1, map.x(a) + rint(41) - 20)), Math.max(0, Math.min(h - 1, map.y(a) + rint(41) - 20)));
      if (i % 11 === 5) b = a;
      const [p0, p1, p2, p3] = getParabolaControlPoints(map, a, b, opt);
      const cp = [p0, p1, p2, p3].flatMap((p: any) => [bits(p.x), bits(p.y)]);
      const len = bits(DistanceBasedBezierCurve.getLength(p0, p1, p2, p3));
      let d: [number, number] | null;
      try {
        d = digest(UniversalPathFinding.Parabola(map, opt).findPath(a, b));
      } catch {
        d = null; // ref() threw: the arc left the map
      }
      const speeds = [1, 2.5, 0.3, 7, 3];
      const speed = i % 3 === 0 ? undefined : speeds[i % speeds.length];
      const pf = UniversalPathFinding.Parabola(map, opt);
      const seq: number[] = [];
      let ok = true;
      let cur = a;
      let last = -1;
      try {
        for (let s = 0; s < 100000; s++) {
          const r = pf.next(cur, b, speed);
          last = r.status;
          if (r.status === PathStatus.NEXT) { cur = r.node; seq.push(cur); } else break;
        }
      } catch {
        ok = false;
      }
      out.parabola.push({ from: a, to: b, opt: opt ?? null, cp, len, d, speed: speed ?? null, walk: ok ? digest(seq) : null, last: ok ? last : null, idx: ok ? pf.currentIndex() : null, full: d ? keep('parabola', UniversalPathFinding.Parabola(map, opt).findPath(a, b), 2, 1500) : undefined });
    }
  }

  // Air paths
  {
    out.air = [];
    for (let i = 0; i < 60; i++) {
      const a = rint(N);
      let b = rint(N);
      if (i % 3 === 0) b = map.ref(Math.max(0, Math.min(w - 1, map.x(a) + rint(401) - 200)), Math.max(0, Math.min(h - 1, map.y(a) + rint(401) - 200)));
      if (i % 10 === 4) b = map.ref(map.x(a), rint(h));
      if (i % 10 === 5) b = map.ref(rint(w), map.y(a));
      const ticks = rint(100000);
      const g = mockGame(map, mini, wm, { ticks });
      const pf = PathFinding.Air(g);
      const full = pf.findPath(a, b);
      const pf2 = PathFinding.Air(g);
      const seq: number[] = [];
      let cur = a;
      for (let s = 0; s < 100000; s++) {
        const r = pf2.next(cur, b);
        if (r.status === PathStatus.NEXT) { cur = r.node; seq.push(cur); } else { if (r.status === PathStatus.COMPLETE) seq.push(r.node); break; }
      }
      out.air.push({ from: a, to: b, ticks, d: digest(full), walk: digest(seq), full: keep('air', full, 2, 1500) });
    }
  }

  // WaterManager: craters, terrain fix-up, throttled rebuild, then queries on the changed graph
  {
    const m2: any = cloneMap(map), mini2: any = cloneMap(mini);
    const wm2 = new WaterManager(m2, mini2, false);
    const ownedTile = pick((t) => m2.isLand(t) && !m2.isImpassable(t));
    m2.setOwnerID(ownedTile, 3);
    const foTile = pick((t) => m2.isLand(t) && !m2.isImpassable(t) && t !== ownedTile);
    m2.setFallout(foTile, true);
    const disk = (cx: number, cy: number, r: number) => {
      const tiles: number[] = [];
      for (let x = cx - r; x <= cx + r; x++)
        for (let y = cy - r; y <= cy + r; y++)
          if (m2.isValidCoord(x, y) && (x - cx) ** 2 + (y - cy) ** 2 <= r * r) tiles.push(m2.ref(x, y));
      return tiles;
    };
    const batches: { tick: number; centers: number[][] }[] = [];
    const nearShore = (r: number) => {
      const s = pick((t) => m2.isShore(t));
      return [m2.x(s) + rint(2 * r + 1) - r, m2.y(s) + rint(2 * r + 1) - r, r];
    };
    const interior = () => {
      for (let tries = 0; tries < 20000; tries++) {
        const t = rint(N);
        if (!m2.isLand(t) || m2.isImpassable(t)) continue;
        const cx = m2.x(t), cy = m2.y(t);
        let ok = true;
        for (let dx = -26; dx <= 26 && ok; dx += 2) for (let dy = -26; dy <= 26; dy += 2) { if (m2.isValidCoord(cx + dx, cy + dy) && m2.isWater(m2.ref(cx + dx, cy + dy))) { ok = false; break; } }
        if (ok) return [cx, cy, 8];
      }
      return nearShore(6);
    };
    batches.push({ tick: 5, centers: [nearShore(8), nearShore(14)] });
    batches.push({ tick: 6, centers: [interior()] });
    batches.push({ tick: 30, centers: [nearShore(20), nearShore(5), nearShore(5), interior()] });
    batches.push({ tick: 31, centers: [nearShore(60)] });
    batches.push({ tick: 33, centers: [nearShore(3), nearShore(40), nearShore(12), nearShore(9), nearShore(25)] });
    batches.push({ tick: 90, centers: [nearShore(10)] });
    const snaps: any[] = [];
    const snap = (tick: number, changed: number[] | null) => {
      const g2 = wm2.miniWaterGraph();
      snaps.push({
        tick,
        changed: changed === null ? null : digest(changed),
        changedFull: changed !== null && changed.length < 40 ? changed : undefined,
        terrain: fnv(m2.terrain),
        miniTerrain: fnv(mini2.terrain),
        landTiles: m2.numLandTiles(),
        miniLandTiles: mini2.numLandTiles(),
        fallout: m2.numTilesWithFallout(),
        waterVersion: m2.waterVersion(),
        graphVersion: wm2.waterGraphVersion(),
        graph: graphDigest(g2),
        ccSample: Array.from({ length: 200 }, (_, i) => g2.getComponentId((i * 2654435761) % MN >>> 0)).reduce((a: number, v: number) => (a * 31 + v) >>> 0, 7),
      });
    };
    snap(0, null);
    const queue: number[] = [];
    for (let t = 1; t <= 130; t++) {
      for (const bt of batches.filter((b) => b.tick === t)) {
        for (const [cx, cy, r] of bt.centers) for (const tile of disk(cx, cy, r)) wm2.queueTile(tile);
        if (t === 5) { wm2.queueTile(ownedTile); wm2.queueTile(foTile); }
        queue.push(t);
      }
      const changed = wm2.tick(t);
      const g3 = wm2.waterGraphVersion();
      if (changed.length > 0 || t === 25 || t === 26 || t === 50 || t === 54 || t === 55 || t === 56 || t === 110 || t === 130 || snaps[snaps.length - 1].graphVersion !== g3) snap(t, changed);
    }
    const game2 = mockGame(m2, mini2, wm2);
    const gameSimple2 = mockGame(m2, mini2, wm2, { noHpa: true });
    const wq: any[] = [];
    for (let i = 0; i < 80; i++) {
      const kind = i % 4;
      const a = kind === 3 ? pick((t) => m2.isWater(t)) : pick((t) => m2.isShore(t));
      const b = kind === 2 || kind === 3 ? pick((t) => m2.isWater(t)) : pick((t) => m2.isShore(t));
      const p = PathFinding.Water(game2).findPath(a, b);
      const ps = i < 20 ? PathFinding.Water(gameSimple2).findPath(a, b) : null;
      wq.push({ from: a, to: b, d: digest(p), ds: i < 20 ? digest(ps) : undefined });
    }
    const wcq = Array.from({ length: 80 }, () => rint(N));
    out.waterManager = {
      ownedTile, falloutTile: foTile,
      batches: batches.map((b) => ({ tick: b.tick, centers: b.centers })),
      snaps, queries: wq,
      waterComponent: wcq.map((t) => [t, wm2.getWaterComponent(t), wm2.getWaterComponentSize(t)]),
    };
  }
  return out;
}

const result = [];
result.push(await generate("Halkidiki", "Compact", 11));
result.push(await generate("World", "Normal", 22));
result.push(await generate("Lemnos", "Normal", 33));
result.push(await generate("Giant World Map", "Normal", 44));
console.log(JSON.stringify(result));
