// SPDX-License-Identifier: AGPL-3.0-only
// Generates PseudoRandomSeedrandom.json: the era S (npm seedrandom, ARC4) PseudoRandom; run with OPENFRONT_WORKTREE at a v0.31.x or v0.32.0-beta1 build.
import { bits, upstream } from "./Common.mts";

const { PseudoRandom } = await upstream("core/PseudoRandom");
const seeds = [
  0, 1, 2, 42, 12345, -1, 4294967301, 1.7, -3.9, 2147483647, 2147483648,
  1e10, 1e21, 1.5e-7, 123456789012345680000, 123456789, -2147483649, 0.5, 987654321,
];
const out: any[] = [];
for (const seed of seeds) {
  const r = (): any => new PseudoRandom(seed);
  let g = r();
  const next = Array.from({ length: 16 }, () => bits(g.next()));
  g = r();
  const long = Array.from({ length: 700 }, () => bits(g.next()));
  g = r();
  const nextInt = Array.from({ length: 16 }, () => g.nextInt(0, 100));
  g = r();
  const nextIntOdd = Array.from({ length: 8 }, () => g.nextInt(-7.5, 20.2));
  g = r();
  const nextFloat = Array.from({ length: 8 }, () => bits(g.nextFloat(-3, 9)));
  g = r();
  const nextID = Array.from({ length: 6 }, () => g.nextID());
  g = r();
  const chance = Array.from({ length: 16 }, () => g.chance(5));
  g = r();
  const items = ["a", "b", "c", "d", "e", "f", "g", "h", "i", "j"];
  const randElement = Array.from({ length: 8 }, () => g.randElement(items));
  g = r();
  const shuffle = g.shuffleArray(Array.from({ length: 12 }, (_, i) => i));
  const shuffleEmpty = g.shuffleArray([]);
  g = r();
  const set = new Set([50, 7, 9, 1000, 3, 12, 4]);
  const randFromSet = Array.from({ length: 8 }, () => g.randFromSet(set));
  g = r();
  const mixed: string[] = [];
  for (let i = 0; i < 10; i++) {
    mixed.push(bits(g.next()), String(g.nextInt(3, 9)), g.nextID());
  }
  out.push({
    seed, next, long, nextInt, nextIntOdd, nextFloat, nextID, chance,
    randElement, shuffle, shuffleEmpty, randFromSet, mixed,
  });
}
console.log(JSON.stringify(out, null, 1));
