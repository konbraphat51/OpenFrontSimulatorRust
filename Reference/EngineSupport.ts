// SPDX-License-Identifier: AGPL-3.0-only
/**
 * Shared helpers of the engine scripts: map loading from disk and record decoding.
 *
 * Copied with the other scripts into `<worktree>/ofsim-reference/`, so the imports resolve to the
 * engine of the record's gitCommit.
 */
import fs from "fs";
import path from "path";
import { fileURLToPath } from "url";
import zlib from "zlib";
import { GameMapType } from "../src/core/game/Game";
import * as mapLoaderModule from "../src/core/game/GameMapLoader";
import type { GameMapLoader, MapData } from "../src/core/game/GameMapLoader";
import type { MapManifest } from "../src/core/game/TerrainMapLoader";
import { createGameRunner } from "../src/core/GameRunner";
import { GameRecord, GameStartInfo } from "../src/core/Schemas";
import * as UtilModule from "../src/core/Util";

// Builds before v0.35.0 that load their config through `getGameLogicConfig` need a bundled game env.
process.env.GAME_ENV ??= "prod";

const { decompressGameRecord } = UtilModule;
// Builds before v0.33.13 have no toWireGameStartInfo: their record info is used as is.
const toWireGameStartInfo: (info: GameStartInfo) => GameStartInfo =
  (UtilModule as unknown as { toWireGameStartInfo?: (info: GameStartInfo) => GameStartInfo })
    .toWireGameStartInfo ?? ((info) => info);

/** Reads map files from the worktree's resources directory. */
export class FileSystemMapLoader implements GameMapLoader {
  constructor(private readonly mapsDirectory: string) {}

  getMapData(map: GameMapType): MapData {
    const key = Object.keys(GameMapType).find(
      (k) => GameMapType[k as keyof typeof GameMapType] === map,
    );
    if (key === undefined) throw new Error(`unknown map: ${map}`);
    const directory = path.join(this.mapsDirectory, key.toLowerCase());
    const readBinary = (name: string) => async () =>
      new Uint8Array(fs.readFileSync(path.join(directory, name)));
    return {
      mapBin: readBinary("map.bin"),
      map4xBin: readBinary("map4x.bin"),
      map16xBin: readBinary("map16x.bin"),
      manifest: async () =>
        JSON.parse(fs.readFileSync(path.join(directory, "manifest.json"), "utf8")) as MapManifest,
      webpPath: path.join(directory, "thumbnail.webp"),
      layerPng: async () => {
        throw new Error("layer images are not needed for simulation");
      },
      layerImage: async () => {
        throw new Error("layer images are not needed for simulation");
      },
    } as unknown as MapData;
  }
}

/** A decoded record with the start info the engine expects. */
export interface LoadedRecord {
  record: GameRecord;
  gameStart: GameStartInfo;
}

/** Reads a raw record (gzip or plain JSON), fills its sparse turns and builds the wire start info. */
export function loadRecord(recordPath: string): LoadedRecord {
  const bytes = fs.readFileSync(recordPath);
  const isGzip = bytes.length > 2 && bytes[0] === 0x1f && bytes[1] === 0x8b;
  const raw = JSON.parse((isGzip ? zlib.gunzipSync(bytes) : bytes).toString("utf8"));
  const record = decompressGameRecord(raw as GameRecord);
  const info = record.info;
  const gameStart = toWireGameStartInfo({
    gameID: info.gameID,
    lobbyCreatedAt: info.lobbyCreatedAt,
    config: info.config,
    players: info.players,
    tribes: info.tribes,
  });
  return { record, gameStart };
}

/** Root of the worktree that contains the calling script. */
export function resolveWorktree(scriptUrl: string): string {
  return path.resolve(path.dirname(fileURLToPath(scriptUrl)), "..");
}

/** Map loader rooted at a worktree. */
export function createMapLoader(worktree: string): FileSystemMapLoader {
  return new FileSystemMapLoader(path.join(worktree, "resources", "maps"));
}

/**
 * Creates the game runner of the worktree's build: v0.35.0 on `createGameRunner` takes the loaded map
 * files (`loadMapFiles`), earlier builds take the map loader.
 */
export async function createRunnerFor(
  gameStart: Parameters<typeof createGameRunner>[0],
  worktree: string,
  callback: Parameters<typeof createGameRunner>[3],
): Promise<Awaited<ReturnType<typeof createGameRunner>>> {
  const loader = createMapLoader(worktree);
  const loadMapFiles = (mapLoaderModule as Record<string, unknown>).loadMapFiles as
    | ((l: GameMapLoader, map: unknown, size: unknown) => Promise<unknown>)
    | undefined;
  const config = gameStart.config as { gameMap: unknown; gameMapSize: unknown };
  const mapArgument = loadMapFiles ? await loadMapFiles(loader, config.gameMap, config.gameMapSize) : loader;
  return createGameRunner(gameStart, undefined, mapArgument as never, callback);
}

/** Splits argv into positional arguments and `--key value` options. */
export function parseArguments(argv: string[]): { positional: string[]; options: Map<string, string> } {
  const positional: string[] = [];
  const options = new Map<string, string>();
  for (let i = 0; i < argv.length; i++) {
    if (argv[i].startsWith("--")) options.set(argv[i], argv[++i]);
    else positional.push(argv[i]);
  }
  return { positional, options };
}

/** Silences the engine's console chatter so stdout and stderr stay readable. */
export function silenceEngineLogs(): void {
  console.debug = () => {};
  console.log = () => {};
  console.info = () => {};
  console.warn = () => {};
}
