// SPDX-License-Identifier: AGPL-3.0-only
/**
 * Golden digests of the record and map layers, computed with the upstream baseline engine code.
 *
 * Responsibility: print, for every baseline record, digests of the expanded turns (decompressGameRecord),
 * the start info (toWireGameStartInfo) and of every map (loadTerrainMap) that the Rust crates
 * `ofsim-record` and `ofsim-map` must reproduce bit for bit. No usernames or game content are printed.
 *
 * Usage: copy this file into `<upstream worktree>/openfrontds/` (so `../src/...` resolves) and run there:
 *   npx tsx openfrontds/GoldenDigests.ts <recordsDir> <outDir>
 * It writes `<outDir>/Records.json` and `<outDir>/Maps.json`. Canonical JSON: keys sorted by UTF-16 code
 * units, null members dropped, JSON.stringify number/string formatting.
 */
import { createHash } from "crypto";
import fs from "fs";
import path from "path";
import { fileURLToPath } from "url";
import zlib from "zlib";
import { GameMapSize, GameMapType } from "../src/core/game/Game";
import { GameMapLoader, MapData } from "../src/core/game/GameMapLoader";
import { MapManifest, loadTerrainMap } from "../src/core/game/TerrainMapLoader";
import { GameRecord, GameStartInfo } from "../src/core/Schemas";
import { decompressGameRecord, toWireGameStartInfo } from "../src/core/Util";

/** Canonical JSON text of a value. */
function canonical(value: unknown): string {
  if (value === null || value === undefined) return "null";
  if (Array.isArray(value)) return "[" + value.map(canonical).join(",") + "]";
  if (typeof value === "object") {
    const record = value as Record<string, unknown>;
    const keys = Object.keys(record)
      .filter((k) => record[k] !== null && record[k] !== undefined)
      .sort();
    return "{" + keys.map((k) => JSON.stringify(k) + ":" + canonical(record[k])).join(",") + "}";
  }
  return JSON.stringify(value);
}

/** Hex sha256 of a string or bytes. */
function sha256(data: string | Uint8Array): string {
  return createHash("sha256").update(data).digest("hex");
}

/** Reads map files from a resources/maps directory (the key is the lowercased enum key). */
class DiskMapLoader implements GameMapLoader {
  constructor(private readonly mapsDirectory: string) {}
  getMapData(map: GameMapType): MapData {
    const key = Object.keys(GameMapType).find((k) => GameMapType[k as keyof typeof GameMapType] === map);
    if (key === undefined) throw new Error(`unknown map: ${map}`);
    const directory = path.join(this.mapsDirectory, key.toLowerCase());
    const bin = (name: string) => async () => new Uint8Array(fs.readFileSync(path.join(directory, name)));
    return {
      mapBin: bin("map.bin"),
      map4xBin: bin("map4x.bin"),
      map16xBin: bin("map16x.bin"),
      manifest: async () => JSON.parse(fs.readFileSync(path.join(directory, "manifest.json"), "utf8")) as MapManifest,
      webpPath: "",
      layerPng: async () => {
        throw new Error("not needed");
      },
    };
  }
}

/** Digest of one loaded map pair. */
async function mapDigest(loader: GameMapLoader, map: GameMapType, size: GameMapSize) {
  const data = await loadTerrainMap(map, size, loader, false);
  const grid = (g: unknown) => {
    const impl = g as { width(): number; height(): number; numLandTiles(): number; terrain: Uint8Array };
    return {
      width: impl.width(),
      height: impl.height(),
      numLandTiles: impl.numLandTiles(),
      terrainSha256: sha256(impl.terrain),
    };
  };
  return {
    map: grid(data.gameMap),
    mini: grid(data.miniGameMap),
    nations: data.nations.length,
    nationsSha256: sha256(canonical(data.nations)),
    additionalNationsSha256: sha256(canonical(data.additionalNations)),
    teamSpawnAreasSha256: sha256(canonical(data.teamGameSpawnAreas ?? null)),
  };
}

/** Digests of one record. */
function recordDigest(file: string) {
  const raw = JSON.parse(zlib.gunzipSync(fs.readFileSync(file)).toString("utf8")) as GameRecord;
  const sparseTurns = raw.turns.length;
  const record = decompressGameRecord(raw);
  const info = record.info;
  const intentCounts: Record<string, number> = {};
  let hashes = 0;
  for (const turn of record.turns) {
    if (turn.hash !== undefined && turn.hash !== null) hashes++;
    for (const intent of turn.intents) intentCounts[intent.type] = (intentCounts[intent.type] ?? 0) + 1;
  }
  const start: GameStartInfo = toWireGameStartInfo({
    gameID: info.gameID,
    lobbyCreatedAt: info.lobbyCreatedAt,
    listed: info.listed,
    config: info.config,
    players: info.players.map((p) => ({
      clientID: p.clientID,
      username: p.username,
      clanTag: p.clanTag,
      isLobbyCreator: p.isLobbyCreator,
      friends: p.friends,
      teamIndex: p.teamIndex,
    })),
    tribes: info.tribes,
  });
  return {
    gameID: info.gameID,
    gitCommit: record.gitCommit,
    gameMap: info.config.gameMap,
    gameMapSize: info.config.gameMapSize,
    sparseTurns,
    numTurns: info.num_turns,
    expandedTurns: record.turns.length,
    hashes,
    turnsSha256: sha256(canonical(record.turns)),
    intentCounts,
    players: start.players.length,
    tribes: start.tribes?.length ?? 0,
    startInfoSha256: sha256(canonical(start)),
  };
}

/** Entry point. */
async function main(): Promise<void> {
  const [recordsDirectory, outDirectory] = process.argv.slice(2);
  if (!recordsDirectory || !outDirectory) throw new Error("usage: GoldenDigests.ts <recordsDir> <outDir>");
  const worktree = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
  const loader = new DiskMapLoader(path.join(worktree, "resources", "maps"));
  fs.mkdirSync(outDirectory, { recursive: true });

  const records: Record<string, unknown> = {};
  for (const name of fs.readdirSync(recordsDirectory).filter((n) => n.endsWith(".json.gz")).sort()) {
    const digest = recordDigest(path.join(recordsDirectory, name));
    records[digest.gameID] = digest;
  }
  fs.writeFileSync(path.join(outDirectory, "Records.json"), JSON.stringify(records, null, 1) + "\n");

  const maps: Record<string, unknown> = {};
  for (const wire of Object.values(GameMapType).sort()) {
    for (const size of [GameMapSize.Normal, GameMapSize.Compact]) {
      maps[`${wire}|${size}`] = await mapDigest(loader, wire, size);
    }
  }
  fs.writeFileSync(path.join(outDirectory, "Maps.json"), JSON.stringify(maps, null, 1) + "\n");
}

void main();
