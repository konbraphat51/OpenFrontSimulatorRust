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
import { GameMapLoader, MapData } from "../src/core/game/GameMapLoader";
import { MapManifest } from "../src/core/game/TerrainMapLoader";
import { GameRecord, GameStartInfo } from "../src/core/Schemas";
import { decompressGameRecord, toWireGameStartInfo } from "../src/core/Util";

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
    };
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
