// SPDX-License-Identifier: AGPL-3.0-only
// Generates GameMap.json: upstream GameMapImpl results on small synthetic terrain buffers.
import { upstream } from "./Common.mts";

const { GameMapImpl, euclDistFN, manhattanDistFN, rectDistFN, isometricDistFN, hexDistFN, andFN } =
  await upstream("core/game/GameMap");

let seed = 12345;
const rnd = () => ((seed = (Math.imul(seed, 1103515245) + 12345) >>> 0), seed >>> 8);

function build(w: number, h: number) {
  const terrain = Array.from({ length: w * h }, () => rnd() & 0xff);
  const land = terrain.filter((b) => b & 0x80).length;
  const gm = new GameMapImpl(w, h, Uint8Array.from(terrain), land);
  const owners: number[][] = [];
  const fallout: number[] = [];
  for (let t = 0; t < w * h; t++) {
    const o = rnd() % 6;
    if (o !== 0) { gm.setOwnerID(t, o); owners.push([t, o]); }
    if (rnd() % 7 === 0) { gm.setFallout(t, true); fallout.push(t); }
    if (rnd() % 11 === 0) gm.setDefenseBonus(t, true);
  }
  const tiles = Array.from({ length: w * h }, (_, t) => t);
  const per = tiles.map((t) => ({
    x: gm.x(t), y: gm.y(t), land: gm.isLand(t), imp: gm.isImpassable(t),
    oceanShore: gm.isOceanShore(t), ocean: gm.isOcean(t), shoreline: gm.isShoreline(t),
    mag: gm.magnitude(t), shore: gm.isShore(t), cost: gm.cost(t), tt: gm.terrainType(t),
    water: gm.isWater(t), n: gm.neighbors(t),
    n8: (() => { const o: number[] = []; gm.forEachNeighborWithDiag(t, (x: number) => o.push(x)); return o; })(),
    n8b: (() => { const o: number[] = []; const k = gm.neighbors8(t, o); return o.slice(0, k); })(),
    edge: gm.isOnEdgeOfMap(t), border: gm.isBorder(t), owner: gm.ownerID(t), hasOwner: gm.hasOwner(t),
    fallout: gm.hasFallout(t), defense: gm.hasDefenseBonus(t), state: gm.tileState(t),
  }));
  const pairs: number[][] = [];
  for (let i = 0; i < 40; i++) {
    const a = rnd() % (w * h), b = rnd() % (w * h);
    pairs.push([a, b, gm.manhattanDist(a, b), gm.euclideanDistSquared(a, b)]);
  }
  const circles: any[] = [];
  const bfss: any[] = [];
  const fns: any[] = [];
  for (let i = 0; i < 12; i++) {
    const t = rnd() % (w * h);
    const r = rnd() % 5;
    circles.push([t, r, [...gm.circleSearch(t, r)], [...gm.circleSearch(t, r, (_: number, d2: number) => d2 % 2 === 1)]]);
    bfss.push([t, [...gm.bfs(t, (g: any, x: number) => g.isLand(x))], [...gm.bfs(t, (g: any, x: number) => g.ownerID(x) === g.ownerID(t))]]);
    for (const center of [false, true]) {
      const mk = { eucl: euclDistFN, manh: manhattanDistFN, rect: rectDistFN, iso: isometricDistFN, hex: hexDistFN };
      const row: any = { t, r, center };
      for (const [name, f] of Object.entries(mk) as any) {
        row[name] = tiles.filter((x) => f(t, r, center)(gm, x));
      }
      row.and = tiles.filter((x) => andFN(euclDistFN(t, r, center), rectDistFN(t, r + 1, center))(gm, x));
      fns.push(row);
    }
  }
  // Mutations.
  const mut: any[] = [];
  const snap = () => ({
    land: gm.numLandTiles(), wv: gm.waterVersion(), nf: gm.numTilesWithFallout(),
    terrain: tiles.map((t) => gm.terrainByte(t)), state: tiles.map((t) => gm.tileState(t)),
  });
  for (let i = 0; i < 10; i++) {
    const t = rnd() % (w * h);
    gm.setWater(t); mut.push({ op: "setWater", args: [t], snap: snap() });
    const t2 = rnd() % (w * h), mag = rnd() & 0xff;
    gm.setMagnitude(t2, mag); gm.setShorelineBit(t2); gm.setOcean(t2);
    mut.push({ op: "magShoreOcean", args: [t2, mag], snap: snap() });
    gm.clearShorelineBit(t2); mut.push({ op: "clearShore", args: [t2], snap: snap() });
    const t3 = rnd() % (w * h);
    const p = (rnd() * 4096 + (rnd() & 0xfff)) >>> 0;
    const ch = gm.updateTile(t3, p); mut.push({ op: "updateTile", args: [t3, p], changed: ch, snap: snap() });
    const t4 = rnd() % (w * h), fo = (rnd() & 1) === 1, ow = rnd() % 4096;
    gm.setFallout(t4, fo); gm.setOwnerID(t4, ow);
    mut.push({ op: "fallOwner", args: [t4, fo ? 1 : 0, ow], snap: snap() });
  }
  return { w, h, terrain, land, owners, fallout, per, pairs, circles, bfss, fns, mut };
}
console.log(JSON.stringify([build(13, 9), build(1, 5), build(5, 1), build(2, 2)]));
