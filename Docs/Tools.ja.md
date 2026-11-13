# 移植支援ツール

クラスごとの移植で消費するトークンと時間を減らす小さなツール群です。標準ライブラリのみの Python 3 または bash で、リポジトリルートはスクリプトの場所から求め、`Cache/` のパスはフラグまたは環境変数で指定します(`Cache/Wt/<name>` の worktree はメインチェックアウトの `Cache/` を使います)。すべて `--help`、既定で簡潔な出力、`--verbose` を備えます。

| ツール | 答える問い |
| --- | --- |
| `Tools/ClassDigest.py <class>` | このクラスで upstream は何を変更し、どの Rust ファイルが移植しているか |
| `Tools/VerifyClass.sh <class>` | クラスの全ビルドの全レコードが通るか |
| `Tools/QuickRegression.sh [class...]` | 移植済みの全クラスの最大レコードがまだ通るか(1 時間ではなく数分) |
| `Tools/BatchSummary.py <ndjson...>` | このバッチ実行で何が起きたか |
| `Tools/Triage.sh <record>` | このレコードはどこで最初に乖離し、どのフラグが影響するか |
| `Tools/Bench.py` | 固定のベンチマークレコードで verify と replay はどれだけ速いか (tick/s、ピーク RSS)。[Performance.ja.md](Performance.ja.md) を参照 |
| `Tools/BuildPgo.sh` | オプトインのプロファイル最適化バイナリ (`target/pgo/release/ofsim`)、0〜8% 高速 |

## ClassDigest

```bash
Tools/ClassDigest.py C064                  # 31 ファイルのクラスで約 470 行
Tools/ClassDigest.py C064 --files-only     # 35 行: ファイル、+/- 行数、Rust 対応
Tools/ClassDigest.py C064 --max-lines 150  # hunk 行の予算を小さくする
```

- `BuildClasses.toml` からクラスと親クラスの先頭コミットを求め、その間の upstream の差分(`--upstream`、`OFSIM_UPSTREAM`、既定 `Cache/OpenFrontIO`)を `BuildPlanGen.py` のエンジンルートに限定し、空白を無視して取ります。
- シミュレーションを変えないファイル(テスト、`*Schemas.ts`、`Stats*.ts`、`GameView.ts`、`GameUpdates.ts`、`UserSettings.ts`、配色テーマ、`Env.ts`、マップローダ、`lang/`、`i18n/`、json、md)は除外し、フッターに列挙します(`--verbose` でフルパス)。クラスが小さすぎる場合はフッターを確認してください。
- hunk をファイルと外側の `Class.method` でまとめます。import、コメント、波括弧のみの行は `--verbose` でのみ表示します。
- `--max-lines N` は hunk 行の総予算で、各ファイルにサイズ比例(最低 4 行)で配分し、切り詰めマーカーを出します。
- `->` の対応は `Crates/` の `//! Port of src/core/<path>.ts` ヘッダから都度作る索引で求めます。`~` はファイル名による代替対応(例 `PlayerExecutionV36.rs`)、`(no Rust port found)` はこのブランチで未移植のファイルです。
- 先頭行にコミット数、コミット間の PR 番号(重複除去)、`Edges/<class>.toml` に既にある `Change` フラグを表示します。

## VerifyClass

```bash
Tools/VerifyClass.sh C040 --jobs 2
```

クラスの全ビルドの `Cache/Records/<commit12>/` に対し、baseline のマップと upstream パスを付けて `ofsim batch --verify-only` を実行し、NDJSON を一時ディレクトリ(`--keep` または `--verbose` がなければ削除)に書き、BatchSummary を表示します。`--quick N` は各ビルドの最大 N レコードだけを使います。ビルドは行わないので、先に `cargo build --profile verify -q` でビルドしてください(`verify` プロファイルは LTO なしの差分ビルドで、編集後のビルドは `--release` の約 66 秒に対し約 12 秒。浮動小数点の意味は同じなのでハッシュは一致します)。ツールは `target/verify/ofsim` があればそれを、なければ `target/release/ofsim` を使います。環境変数: `OFSIM_BIN`、`OFSIM_MAPS`、`OFSIM_UPSTREAM`、`OFSIM_RECORDS`、`OFSIM_CACHE`、`OFSIM_TMP`。

## QuickRegression

```bash
Tools/QuickRegression.sh                 # BuildClasses.toml の全クラス、ビルドごとに 1 レコード
Tools/QuickRegression.sh --per-build 2 C000 C040
```

クラスごとに要約 1 行と ok 以外のレコードを表示し、未移植のクラスは最後にまとめて列挙します。マージやリファクタリングの後に使い、公開前には全レコード(クラスごとの `VerifyClass.sh`)を流します。

## upstream worktree

`Reference/Setup.sh <commit>` は `Cache/Worktrees/<commit12>` を作り、`node_modules` をプール `Cache/NodeModules/<package-lock のハッシュ>/` からリンクします(lock ファイルごとに `npm ci` は 1 回)。worktree を削除しても他が壊れません。削除は `git -C Cache/OpenFrontIO worktree remove --force Cache/Worktrees/<commit12>`。

## BatchSummary

```bash
Tools/BatchSummary.py batch.ndjson [--by-build Cache/Records]
```

合計 1 行(`records ok desync error hashes`)、ok でないレコードごとに 1 行(レコード、状態、最初の tick またはエラー、`--width` で切り詰め、既定 120)、`--by-build` ではコミットとクラスごとに 1 行を出力します。引数なしなら標準入力を読みます。

## Triage

```bash
Tools/Triage.sh Cache/Records/<commit12>/<id>.json.gz [--trace] [--bisect-flags [--depth 2]] [--until T]
```

- 既定: コミット、クラス、判定、一致したハッシュ数、最初の不一致 tick を 1 行で表示します。
- `--trace`: その tick まで `Tools/TraceCompare.sh` を実行し、最後の 20 行(最初に異なるフィールド)を表示します。`OFSIM_WORKTREE`(レコードのコミットの upstream worktree。`node_modules` と最新の `ofsim-reference/` が必要)を指定します。
- `--bisect-flags`: クラスの `Edges/<class>.toml` の `Change` フラグ(`--depth 2` で親の分も)ごとに `OFSIM_TOGGLE=<flag>` で verify をやり直し、1 行ずつ `REMOVES mismatch`、`MOVES a -> b`、`no effect`(レコードが通る場合は `BREAKS at tick`)を表示します。`--until T` で高速化できます。
- 注意: `OFSIM_TOGGLE` は `Rules::for_class` で適用され、C000 自身のビルドに対する C000 のフラグの反転は効果がありません。
