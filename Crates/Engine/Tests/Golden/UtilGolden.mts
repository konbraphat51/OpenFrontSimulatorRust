// Generates Util.json: simpleHash, toInt, within, sigmoid, manhattanDistWrapped and getMode from upstream Util.ts.
import { bits, upstream } from "./Common.mts";

const U = await upstream("core/Util");
const G = await upstream("core/game/Game");
const strings = [
  "", "a", "abc", "hello world", "Player 1", "u7Qx9KaB", "ZZZZZZZZ",
  "日本語のなまえ", "😀😎", "a😀b", "\u{10000}\u{10ffff}", "x".repeat(200),
  "\u0000￿", "The quick brown fox jumps over the lazy dog",
];
for (let i = 0; i < 60; i++) strings.push(`id-${i * 7919}-${(i * 104729).toString(36)}`);
const ints = [0, 1.5, -1.5, 2.999999, -0.0, 1e15, -1e15, 123456789.987, Infinity, -Infinity, 9007199254740991];
const wrapped = [
  [0, 0, 10, 0, 100], [0, 0, 99, 0, 100], [5, 5, 95, 3, 100], [50, 1, 0, 9, 100],
].map(([x1, y1, x2, y2, w]) => [x1, y1, x2, y2, w, U.manhattanDistWrapped(new G.Cell(x1, y1), new G.Cell(x2, y2), w)]);
const sig: number[][] = [];
for (const v of [-5, 0, 3, 10, 560, 12.5, 1e5, 300000])
  for (const [d, m] of [[2.5, 12.6], [Math.LN2 / 100, 560], [0.3, 4]])
    sig.push([v, d, m]);
const counts = new Map<string, number>([["a", 1], ["b", 3], ["c", 3], ["d", 2]]);
console.log(
  JSON.stringify({
    hash: strings.map((s) => [s, U.simpleHash(s)]),
    toInt: ints.map((x) => [bits(x), U.toInt(x).toString()]),
    within: [[5, 0, 3], [-1, 0, 3], [2, 0, 3], [0.5, 0.6, 2], [3, 0.82, 7.5]].map((a) => [...a, bits(U.within(a[0], a[1], a[2]))]),
    sigmoid: sig.map(([v, d, m]) => [bits(v), bits(d), bits(m), bits(U.sigmoid(v, d, m))]),
    wrapped,
    mode: U.getMode(counts),
    display: [["bob", null], ["bob", ""], ["bob", "AB"]].map(([n, t]) => [n, t, U.formatPlayerDisplayName(n, t)]),
  }),
);
