// SPDX-License-Identifier: AGPL-3.0-only
// Dumps the data of the `obscenity` English matcher that upstream's username censoring uses to
// Crates/Engine/data/Obscenity.json. Usage: node Tools/GenObscenityData.mjs <upstream worktree with node_modules> <out.json>
import { createRequire } from "node:module";
import fs from "node:fs";
import path from "node:path";

const [worktree, out] = process.argv.slice(2);
const require = createRequire(path.join(path.resolve(worktree), "package.json"));
const o = require("obscenity");
const dist = path.join(path.dirname(require.resolve("obscenity")));
const confusables = require(path.join(dist, "transformer/resolve-confusables/confusables.js")).confusables;
const leet = require(path.join(dist, "transformer/resolve-leetspeak/dictionary.js")).dictionary;

const data = o.englishDataset.build();
const node = (n) => {
  if (n.kind === 2) return { k: "lit", chars: n.chars };
  if (n.kind === 0) return { k: "opt", child: node(n.childNode) };
  return { k: "wild" };
};
/** char -> canonical char pairs of a `remapCharactersTransformer` mapping. */
const remap = (mapping) => {
  const pairs = new Map();
  for (const [original, equivalents] of mapping instanceof Map ? mapping.entries() : Object.entries(mapping)) {
    for (const eq of equivalents) pairs.set(eq.codePointAt(0), original.codePointAt(0));
  }
  return [...pairs.entries()];
};
const json = {
  blacklist: data.blacklistedTerms.map((t) => ({
    start: t.pattern.requireWordBoundaryAtStart,
    end: t.pattern.requireWordBoundaryAtEnd,
    nodes: t.pattern.nodes.map(node),
  })),
  whitelist: data.whitelistedTerms,
  confusables: remap(confusables),
  leet: remap(leet),
  collapse: { default: 1, custom: [["b", 2], ["e", 2], ["o", 2], ["l", 2], ["s", 2], ["g", 2]].map(([c, n]) => [c.codePointAt(0), n]) },
};
fs.writeFileSync(out, JSON.stringify(json));
console.log(json.blacklist.length, json.whitelist.length, json.confusables.length, json.leet.length);
