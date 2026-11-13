# リプレイ出力

English version: [Output.md](Output.md)

このファイルの責務: `ofsim replay` と `ofsim batch` が書くファイルと、OpenFrontDs プロジェクトのアップストリーム版ダンパー (`Engine/ReplayDumper.ts`) との関係。利用側がエンジンを切り替えられるようにするためのものです。

## ファイル

`ofsim replay --record R --maps M --out DIR --every N` は `DIR` に次を書きます。

| ファイル | 内容 |
| --- | --- |
| `Tiles/T{tick:06}.u16.zst` | リトルエンディアン `u16` のタイル状態 (`width * height` 個、行優先) の zstd フレーム (レベル 3、内容サイズあり、チェックサムなし)。ビット 0〜11 が所有者の small id、ビット 13 が fallout、ビット 14 が防御ボーナス。`tick` はその tick の実行後の `ticks()`。 |
| `Entities.ndjson` | スナップショットごとに 1 行の JSON: `tick`, `inSpawnPhase`, `players`, `units`, `attacks`, `alliances` (フィールド名と順序は `ReplayDumper.ts` と同じ)。人間プレイヤーは `name: null`、ネーションとボットは名前を出力、`clientId` はアップストリームと同様に出力。 |
| `Result.json` | `status` (`ok`, `desync`, `error`), `gameID`, `gitCommit`, `finalTick`, `hashesChecked`, `firstMismatchTick`, `width`, `height`, `snapshotEvery`, `snapshotTicks`, `error`。 |

スナップショットは tick 実行後に `ticks() % N == 0` のとき、および最終ターンの後に 1 回 (ハッシュ不一致の後も。エラー後は除く) 取ります。記録されたハッシュはエンジンがハッシュを計算する tick (10 tick ごと) で比較し、不一致で `status: desync` として停止します。

## アップストリームのダンパーとの互換性

`Tools/CompareReplayOutput.py <アップストリームの出力> <ofsim の出力>` でベースラインの 3 レコード (`cLVr8coTsN`, `cj1KGD54xA`, `Q94sNurv`、`--every 100`) を確認済み。`Result.json`、`Entities.ndjson`、すべてのタイルフレームがバイト単位で一致しました。

意図した差: 失敗時の `error` 文字列 (JS のスタックではなく Rust の panic メッセージと位置)。zstd フレームは libzstd のバージョンが同じ場合のみ一致が保証されます (ここでは両方 1.5.7)。

## バッチ

`ofsim batch --records <dir|glob> --maps M --out DIR [--every N] [--jobs J] [--verify-only] [--force]`

- `--records` はディレクトリ (すべての `*.json`/`*.json.gz`) または `*` と `?` を使う `dir/pattern`。
- 各レコードは `replay` と同じく `DIR/<name>/` に書きます。`Result.json` が既にあるレコードは `--force` がない限り `skipped` になります。
- `--verify-only` は何も書かず、常に実行します。
- レコード単位で並列 (`--jobs`、既定はコア数。大きいレコードから先に開始し、各ワーカーが共有カウンタから次のレコードを取る)。1 レコードの panic やエラーはその行に報告され、他のレコードは止まりません。
- 標準出力にレコードごとに 1 行の JSON: `record`, `status` (`ok`, `desync`, `error`, `skipped`), `ticks`, `seconds`, `ticksPerSecond`、可能なら `hashesChecked`, `snapshots`, `firstMismatchTick`, `error`。
- `ok` でも `skipped` でもないレコードがあると終了コード 1。
