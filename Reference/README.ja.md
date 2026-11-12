# リファレンストレーサー

English version: [README.md](README.md)

このファイルの責務: アップストリームの TypeScript エンジンをリファレンストレーサーとして実行する方法と、`ofsim trace` が再現しなければならないトレース形式の正確な定義。

## セットアップ

```sh
Reference/Setup.sh 4e837520e886          # Linux/macOS。Windows: Reference\Setup.ps1 4e837520e886
```

`Cache/OpenFrontIO` がなければアップストリームを clone し、ワークツリー `Cache/Worktrees/<commit12>` を追加し、`npm ci --ignore-scripts --engine-strict=false` を実行し (アップストリームは Node 24 を要求するが Node 22 でも動く)、`Reference/*.ts` を `<worktree>/ofsim-reference/` にコピーする。スクリプトを編集したら再実行する。冪等である。スクリプトは `../src/core/...` を import するため、常にそのワークツリーのコミットのエンジンに対して動く (レコードの `gitCommit` に対応するワークツリーを使うこと)。

## TraceDumper

```sh
cd Cache/Worktrees/<commit12>
npx tsx ofsim-reference/TraceDumper.ts <record.json[.gz]> <out.ndjson> [--full-at t1,t2,...] [--until tick]
```

クライアント/ワーカーと同じ方法でレコードを再生し、1 tick につき 1 行の NDJSON を書く。サマリ JSON (tick 数、検証したハッシュ数、不一致数、秒数、ticks/s) は stderr に出る。記録ハッシュが 1 つでも不一致なら終了コードは 2。`--until T` は tick `T` (含む) で停止する。`--full-at` は指定 tick ごとに 1 行を `<out.ndjson>.full.ndjson` に追加で書く。エンジンの console 出力はユーザー名を含むため抑止される。

### レコードからエンジンへの配線

1. `decompressGameRecord(raw)` (`src/core/Util.ts`) が疎なターンを埋めるので、`record.turns[i].turnNumber == i`。
2. `toWireGameStartInfo({gameID, lobbyCreatedAt, config, players, tribes})` (`src/core/Util.ts`) が `GameStartInfo` を作る。
3. `createGameRunner(gameStart, undefined, mapLoader, callback)` (`src/core/GameRunner.ts`) がゲームを構築し `init()` を呼ぶ。
4. 各ターンを順に: `runner.addTurn(turn)`、続けて `runner.executeNextTick()`。

### トレース形式

1 行に 1 つの JSON オブジェクト。tick 順で tick 0 から始まり、レコードの全ターンに 1 行ずつ。

```json
{"tick":530,"hash":13238919558137502,"recordHash":13238919558137502,"players":[[1,25000,0,"0",0,false,0]]}
```

| フィールド | 意味 |
| --- | --- |
| `tick` | tick 実行**前**の `game.ticks()`。レコードのターン番号 (`turn.turnNumber`) に等しい。tick `t` はターン `t` を消費し、その後 `ticks()` は `t + 1` になる。 |
| `hash` | tick `t` の `executeNextTick()` が戻った**後**に、毎 tick 評価した `GameImpl.hash()`。値は `1 + (Map の挿入順での各プレイヤーの Player.hash() の総和)`。`Player.hash() = simpleHash(playerId) * (troops + numTilesOwned) + unitHashSum`、`Unit.hash() = tile + simpleHash(unitType) * unitId`。整数値の JS `number` (f64) で 2^53 を超えうる。指数表記も小数点もない 10 進整数で書き、f64 として比較する。 |
| `recordHash` | サーバーが書いたレコードのターン `t` の `turn.hash`。なければ `null` (10 ターンごとに 1 つある)。 |
| `players` | `game.allPlayers()` の各要素 (人間、ネーション、部族を挿入順に。生死問わず) につき 1 配列: `[smallId, troops, tilesOwned, gold, unitHashSum, isAlive, numUnits]`。`troops` は JS と同じ number (整数値の f64、小数点なしで出力)、`tilesOwned` は整数、`gold` は 10 進**文字列** (bigint)、`unitHashSum` はそのプレイヤーの有効なユニットの `Unit.hash()` の総和 (プレイヤーハッシュのユニット部分)、`isAlive` は真偽値、`numUnits` はそのユニット数。 |

読み取り時点: アップストリームは保存されるハッシュを `GameImpl.executeNextTick()` 内で `ticks() % 10 == 0` のとき、実行と player update の後、water manager と `_ticks++` の前に計算する。トレーサーは tick 全体の後に読むが、water manager はタイル状態にしか触れず兵力・保有タイル数・ユニットには触れないので一致する。`recordHash` がある tick では `hash == recordHash` が成り立たなければならず、ダンパーは違反数を数える。エンジンが出す `Hash` update は使わない (private な `hash()` を直接呼ぶ)。

### 完全ダンプ (`--full-at`)

`<out.ndjson>.full.ndjson` に、指定した各 tick について `{"tick":t,"full":{...}}` を書く。読み取り時点はトレース行と同じ。内容: `width`、`height`、`tileState` (タイルごとの生の `u16`、ビット 0-11 が所有者)、`owners` (タイルごとの所有者 small id)、`units` (`id,type,owner,tile,troops,level,active,hash`、`game.units()` の順)、`players` (`smallId,type,troops,gold,tilesOwned,isAlive,hasSpawned,hash`)。大きい: 大きなマップの 533 tick のゲームで約 16 MB。

### Inert モードとシナリオ

`--inert 1` (`InertPatch.ts`) は、Rust エンジンが未移植の Execution (`PORTED_INTENTS` 以外の intent、`NationExecution`、`TribeExecution`、`DoomsdayClockExecution`) をアップストリームのエンジンでも無効化する。`ofsim` は `OFSIM_INERT=1` で同じ動作になり、`Tools/TraceCompare.sh <record> <until> --inert` で比較する。このトレースはレコードのハッシュとは一致しない。`ScenarioDump.ts` はシード付きの領土ファズ (占領、放棄、囲い込み) を実行し、`ofsim scenario` と比較する。

作業中のブランチでは、追加で移植済みの intent 種別を `OFSIM_PORTED_INTENTS` (カンマ区切り、例 `attack,boat`)、有効のままにするエンジン生成の Execution を `OFSIM_PORTED_EXECS` (`Nation`、`Tribe`、`DoomsdayClock`) で指定する。これで共有ファイルを変更せずに済む。移植済み Execution の内部から生成される Execution (例: tribe が出す攻撃) は対象外なので、両側で移植済みにするか、最初の発生までだけを比較する。

## TraceDiff

```sh
npx tsx ofsim-reference/TraceDiff.ts a.ndjson b.ndjson
```

最初に異なる行 (tick)、異なるハッシュのフィールド、最初に異なるプレイヤーとフィールド名を出力する。同一なら終了コード 0、異なれば 1。

## FetchRecords

```sh
python Tools/FetchRecords.py --start 2026-10-03T00:00:00Z --end 2026-10-03T00:29:59Z \
  --type Public --mode "Free For All" --commit 4e837520e886 --max 5
```

ウィンドウの `/public/games` を列挙し (`--type`、`--mode` で絞り込み)、`/public/game/<id>` を `Cache/Records/<commit12>/<gameId>.json.gz` にリクエスト間隔 0.5 秒以上でダウンロードし、`gitCommit` が `--commit` で始まるレコードだけを残す (コミットはダウンロード後にしか分からない)。HTTP は `curl` 経由 (別のトランスポートは `http_get` を差し替える)。`--exclude-day` (複数指定可、既定 `2026-10-02` UTC) の日に開始したゲームは決してダウンロードしない。親研究プロジェクトのホールドアウトデータである。レコードにはユーザー名が含まれるため、コミットしないこと。
