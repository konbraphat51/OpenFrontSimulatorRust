// SPDX-License-Identifier: AGPL-3.0-only
// Generates DetMath.json: DetMath functions on a grid of inputs including edge cases.
import { bits, upstream } from "./Common.mts";

const D = await upstream("core/DetMath");
const specials = [
  0, -0, 1, -1, 0.5, 2, 10, 100, 1e-300, 5e-324, 2.2250738585072014e-308,
  1e300, 709, 709.5, -708, -708.5, -1022, -1023, 1023, 1024, 1e-9,
  Math.LN2, Math.PI, Math.E, Infinity, -Infinity, NaN, 0.41421356237309503,
  0.4142135623730951, 1.4142135623730951, 1.4142135623730954,
];
const grid: number[] = [...specials];
for (let i = -60; i <= 60; i++) grid.push(i * 0.37 + 0.011);
for (let i = 1; i <= 80; i++) grid.push(i * i * 1.7);
for (let i = -700; i <= 700; i += 17) grid.push(i + 0.25);
const rec = (f: (x: number) => number) =>
  grid.map((x) => [bits(x), bits(f(x))]);
const bin = (f: (x: number, y: number) => number, xs: number[], ys: number[]) => {
  const rows: string[][] = [];
  for (const x of xs) for (const y of ys) rows.push([bits(x), bits(y), bits(f(x, y))]);
  return rows;
};
const bases = [...specials.filter((x) => x !== Infinity), 3, 7, 0.3, 123456.789, 25000, 1e6];
const exps = [0, 0.6, 0.73, -1, 1, 2, 0.5, -0.03, 1e-5, 30, -30, 700, NaN];
const angles = [0, -0, 1, -1, 0.3, -0.3, 5, -5, 1e-9, 1e9, Infinity, NaN, 0.4142135623730951];
console.log(
  JSON.stringify({
    pow2: rec(D.pow2),
    exp: rec(D.exp),
    log: rec(D.log),
    pow: bin(D.pow, bases, exps),
    atan2: bin(D.atan2, angles, angles),
  }),
);
