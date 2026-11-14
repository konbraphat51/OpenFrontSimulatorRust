// SPDX-License-Identifier: AGPL-3.0-only
// Generates Crates/Engine/data/*.json from an upstream worktree (name tables the simulation draws from).
// Usage (from the upstream worktree): npx tsx <repo>/Tools/GenEngineData.ts <worktree> <outDir>
import * as fs from "node:fs";
import * as path from "node:path";

const [worktree, outDir] = process.argv.slice(2);
// From v0.35.0 the engine sources live in packages/*/src (monorepo split); find a module in either layout.
const locate = (module: string): string => {
  const candidates = [path.join(worktree, "src/core", module)];
  for (const pkg of fs.existsSync(path.join(worktree, "packages")) ? fs.readdirSync(path.join(worktree, "packages")) : []) {
    candidates.push(path.join(worktree, "packages", pkg, "src", module));
  }
  const found = candidates.find((candidate) => fs.existsSync(candidate));
  if (found === undefined) throw new Error(`module not found: ${module}`);
  return found;
};

// Nation names: NAME_TEMPLATES and NOUNS are module-private, so they are evaluated from the source text.
const src = fs.readFileSync(locate("game/NationCreation.ts"), "utf8");
const slice = (start: string, end: string): string => {
  const from = src.indexOf(start);
  const to = src.indexOf(end, from);
  return src.slice(from + start.length, to);
};
const templatesText = slice("const NAME_TEMPLATES: NameTemplate[] = [", "\n];");
const nounsText = slice("const NOUNS = [", "\n];");
const PLURAL_NOUN = "$PLURAL";
const NOUN = "$NOUN";
const templates = new Function("PLURAL_NOUN", "NOUN", `return [${templatesText}];`)(PLURAL_NOUN, NOUN);
const nouns = new Function(`return [${nounsText}];`)();
fs.writeFileSync(path.join(outDir, "NationNames.json"), JSON.stringify({ templates, nouns }));

// Tribe names: themes plus per-map theme lists and custom tribes.
const themes = JSON.parse(fs.readFileSync(path.join(worktree, "resources/tribeNameThemes.json"), "utf8"));
const mapsGen = await import(locate("game/Maps.gen.ts"));
const maps: Record<string, unknown> = {};
for (const m of mapsGen.maps) {
  maps[m.type] = { themes: m.themes ?? null, customTribes: m.customTribes ?? null };
}
fs.writeFileSync(path.join(outDir, "TribeNames.json"), JSON.stringify({ themes, maps }));
console.log("templates", templates.length, "nouns", nouns.length, "maps", Object.keys(maps).length);
