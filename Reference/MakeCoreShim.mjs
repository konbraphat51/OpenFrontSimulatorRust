// SPDX-License-Identifier: AGPL-3.0-only
/**
 * Creates `<worktree>/src/core/` re-export shims for builds whose engine moved to `packages/*` (v0.35.0 on).
 *
 * The reference scripts import baseline paths (`../src/core/game/Game`); on a monorepo-split build each
 * such module is re-exported from the package file that now holds it. Does nothing when `src/core` exists.
 *
 * Usage: node ofsim-reference/MakeCoreShim.mjs <worktree> <reference-script-directory>
 */
import fs from "fs";
import path from "path";

/** Modules whose exports are split over several package files; extra sources are re-exported too. */
const EXTRA_SOURCES = {
  "game/Game": ["engine-api/game/GameTypes", "engine-api/game/Maps.gen"],
  "game/GameMapLoader": ["shared/GameMapLoader"],
  "game/TerrainMapLoader": ["engine-lib/game/TerrainMapLoader", "engine-api/game/MapFiles"],
  Schemas: ["engine-api/Schemas", "shared/WireSchemas"],
  Util: ["engine-lib/Util", "shared/SharedUtil"],
};

const [worktree, scriptsDirectory] = process.argv.slice(2);
const coreDirectory = path.join(worktree, "src", "core");
const packagesDirectory = path.join(worktree, "packages");
if (!fs.existsSync(packagesDirectory) || fs.existsSync(path.join(worktree, "src", "core", "Schemas.ts"))) {
  process.exit(0);
}

/** Finds `<package>/src/<module>.ts` for a baseline module path, or null. */
function findInPackages(modulePath) {
  for (const entry of fs.readdirSync(packagesDirectory)) {
    if (fs.existsSync(path.join(packagesDirectory, entry, "src", `${modulePath}.ts`))) {
      return `${entry}/${modulePath}`;
    }
  }
  return null;
}

const wanted = new Set();
for (const name of fs.readdirSync(scriptsDirectory)) {
  if (!name.endsWith(".ts")) continue;
  const text = fs.readFileSync(path.join(scriptsDirectory, name), "utf8");
  for (const match of text.matchAll(/["']\.\.\/src\/core\/([^"']+)["']/g)) wanted.add(match[1]);
}

for (const modulePath of wanted) {
  const sources = [];
  const primary = findInPackages(modulePath);
  if (primary) sources.push(primary);
  sources.push(...(EXTRA_SOURCES[modulePath] ?? []));
  if (sources.length === 0) {
    console.error(`MakeCoreShim: no package file for ${modulePath}`);
    continue;
  }
  const lines = [...new Set(sources)].map((s) => {
    const [pkg, ...rest] = s.split("/");
    return `export * from "@openfront/${pkg}/${rest.join("/")}";`;
  });
  const target = path.join(coreDirectory, `${modulePath}.ts`);
  fs.mkdirSync(path.dirname(target), { recursive: true });
  fs.writeFileSync(target, `${lines.join("\n")}\n`);
}
