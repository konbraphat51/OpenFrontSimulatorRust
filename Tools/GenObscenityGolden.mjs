// SPDX-License-Identifier: AGPL-3.0-only
// Writes Crates/Engine/tests/Golden/Obscenity.json: hasMatch results of the upstream matcher for generated names.
// Usage: node Tools/GenObscenityGolden.mjs <upstream worktree with node_modules> <out.json>
import { createRequire } from "node:module";
import fs from "node:fs";
import path from "node:path";

const [worktree, out] = process.argv.slice(2);
const require = createRequire(path.join(path.resolve(worktree), "package.json"));
const o = require("obscenity");
const matcher = new o.RegExpMatcher({ ...o.englishDataset.build(), ...o.englishRecommendedTransformers });
const data = o.englishDataset.build();
let seed = 12345;
const rnd = (n) => {
  seed = (seed * 1103515245 + 12345) & 0x7fffffff;
  return seed % n;
};
const leet = { a: "@4", c: "(", e: "3", i: "1|!", g: "6", o: "0", s: "$5", t: "7", z: "2" };
const literal = (n) => (n.kind === 2 ? String.fromCodePoint(...n.chars) : n.kind === 0 ? (rnd(2) ? literal(n.childNode) : "") : "x");
const cases = new Set();
const noise = ["", " ", "_", "xx", "-", "Z", "[AB]", "1", "..", "\u{1F600}", "a b"];
for (const term of data.blacklistedTerms) {
  for (let k = 0; k < 6; k++) {
    let w = term.pattern.nodes.map(literal).join("");
    w = [...w].map((c) => (rnd(4) === 0 && leet[c] ? leet[c][rnd(leet[c].length)] : rnd(5) === 0 ? c.toUpperCase() : c)).join("");
    if (rnd(3) === 0) w = [...w].map((c) => (rnd(4) === 0 ? c + c : c)).join("");
    cases.add(noise[rnd(noise.length)] + w + noise[rnd(noise.length)]);
  }
}
for (const w of data.whitelistedTerms) cases.add(w), cases.add(noise[rnd(noise.length)] + w.toUpperCase());
for (const w of ["Alice", "Hello World", "Class", "assist", "Bob the Builder", "scunthorpe", "p4ssw0rd", "NicePeopleOnly", "xX_Gamer_Xx", "Ｆｕｌｌ", "ａｓｓ"]) cases.add(w);
const list = [...cases].map((s) => [s, matcher.hasMatch(s)]);
fs.writeFileSync(out, JSON.stringify(list));
console.log(list.length, list.filter((x) => x[1]).length);
