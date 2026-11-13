# 性能

English version: [Performance.md](Performance.md)

このファイルの責務: スループットの測定方法、現在のベンチマーク表、P6-P 最適化パスの変更内容、残りの課題。すべての変更に共通の不変条件は、状態ハッシュが一切変わらないこと (反復順序と浮動小数点演算の順序を保つ)。`Tools/QuickRegression.sh` で確認し、公開前に移植済み全クラスで `Tools/VerifyClass.sh` を実行する。

## 測定方法

```bash
cargo build --release                      # fat LTO、約 90 秒
Tools/Bench.py                             # verify と replay、固定の長いレコード 7 件 (C000、C023、C040、C049、C059、C081、C088)
Tools/Bench.py --mode verify --only C081   # 1 世代だけ
Tools/Bench.py --runs 3 --json out.json    # 3 回の最速値、機械可読
Tools/Bench.py --scale                     # セット全体で `ofsim batch --jobs 1,2,4` も計測
Tools/Bench.py path/to/record.json.gz      # 任意のレコード
```

- 各測定はレコード 1 件につき `ofsim batch --jobs 1` を 1 プロセス実行するため、数値はシングルスレッドの tick/s (エンジンとレコード読み込み約 0.1 秒を含む) と、そのプロセスのピーク RSS。`replay` は 100 tick ごと (`--every`) のスナップショットを一時ディレクトリに書く。
- 環境変数: `OFSIM_BIN` (既定 `target/release/ofsim`)、`OFSIM_MAPS`、`OFSIM_UPSTREAM`、`OFSIM_RECORDS`、`OFSIM_CACHE`。
- 共有 VM のため実行時間は実行ごとに 5〜10% 変動する。変更の前後比較には決定的な命令数を使う: `valgrind --tool=callgrind --callgrind-out-file=cg.out target/release/ofsim verify --record R --maps M --until 3000` の後 `callgrind_annotate cg.out` (自己コスト) または `--inclusive=yes`。release プロファイルは行テーブルを残しているので `--auto=yes` で熱い行が見える。
- `Tools/BuildPgo.sh` はプロファイル最適化ビルドを `target/pgo/release/ofsim` に作る (`rustup component add llvm-tools` が必要)。C081 で実行により 0〜8% 高速化 (VM が不安定)、ハッシュは変わらない。`-C target-cpu=x86-64-v3` は計測したが効果がなかった (分岐とメモリで律速) ため使わない。

## ベンチマーク (release プロファイル、1 スレッド、4 コア共有 VM)

| クラス | レコード | tick 数 | 前 (verify tick/s) | 後 | 高速化 | 前 (replay) | 後 | ピーク RSS 前から後 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| C000 | dqD3tRRz64 | 11,102 | 423 | 516 | 1.22x | 404 | 510 | 536 から 354 MB |
| C023 | tEJEgWSc | 12,932 | 318 | 397 | 1.25x | 298 | 403 | 921 から 628 MB |
| C040 | HQ4dbm2d | 4,362 | 413 | 535 | 1.30x | 410 | 554 | 343 から 255 MB |
| C049 | FMHzJ8XF | 12,875 | 718 | 888 | 1.24x | 687 | 808 | 472 から 323 MB |
| C059 | 67986TJw | 7,002 | 244 | 388 | 1.59x | 239 | 384 | 525 から 376 MB |
| C081 | cjPSE6enhb | 10,712 | 519 | 655 | 1.26x | 512 | 641 | 413 から 285 MB |
| C088 | cxzGcV5tJm | 9,022 | 291 | 388 | 1.33x | 285 | 403 | 516 から 362 MB |
| 全体 | | 68,007 | 387 | 506 | 1.31x | 373 | 504 | |

「前」は統合ブランチ `aa545d3`。最初の 3000〜5000 tick の命令数は 25〜40% 減った (C081 31.5 G から 22.8 G、C000 39.6 G から 28.9 G、C088 は 5000 tick で 86.1 G から 54.2 G)。実時間の減りが小さいのは、残りの時間がタイルセットとタイル配列のキャッシュミスに支配されるため。以前のアップストリーム比 (別のレコードで 3.3〜3.8 倍) と合わせると、シングルスレッドでアップストリームの約 4.3〜5 倍と見られる (アップストリームは再計測していない)。

スナップショット: `ofsim replay` はタイルフレームの圧縮と書き込みを 3 本のバックグラウンドスレッドで行う (`batch` ではジョブごとに 1 本)。HQ4dbm2d を `--every 1` で処理した場合、59.8 秒から 33.9 秒になった。残りは zstd レベル 3 そのもの (フレームはアップストリームのダンパーとバイト単位で一致させる必要があるため、レベルは変えられない)。

## P6-P パスでの変更

| 変更 | 場所 | 効果 |
| --- | --- | --- |
| `Rules::has` を `BTreeSet` 検索からビットセットへ | `Builds/Rules.rs` | C000/C081 で 10〜12% (すべての隣接走査にあった) |
| 検索専用マップに FxHash (Legacy28 `SerialAStar`、海岸補正、BFS、空間クエリ) | `Compat/OrderedMap.rs` (`FxHashMap`、`FxHashSet`)、`Pathfinding/` | C059 で -35% (SipHash が命令数の 40% を占めていた) |
| `OrderedMap::insert` のハッシュ操作を 1 回に、スロット索引を `u32` に | `Compat/OrderedMap.rs` | 命令数減、タイルセットのテーブルが半分 (RSS -25%) |
| 除算なしの `x(tile)`/`y(tile)` (Lemire fastmod、幅とタイルを網羅したテスト付き) | `Game/GameMap.rs` | すべての隣接走査と A* で削減 |
| ユニット種別ごとの個数で `has_unit_nearby`、`nearby_units`、`any_unit_nearby` を短絡 | `Game/UnitGrid.rs` | 防衛拠点がないとき、征服タイルごとの走査を省く |
| タイルごとの `border_owner`: 境界集合の add/delete は状態が変わったときだけ | `Game/GameImpl.rs` | 征服 1 回あたりのハッシュ操作削減 |
| `FlatBinaryHeap` の要素を 1 配列に詰める | `Execution/Utils/FlatBinaryHeap.rs` | 約 2% |
| `PlayerExecutionV36` の領土フラッドを行スライス付きスキャンラインに | `Execution/PlayerExecutionV36.rs` | C088 で命令数 -20% (この世代ではフラッドが 1 tick あたり約 10 万タイルを訪れる) |
| 削除された攻撃の境界集合を解放 | `Game/AttackImpl.rs` | 長いゲームで RSS -60 MB |
| `DetMath::exp` の libm なし `floor`、`can_see` の地形読み出し 1 回、フラッド述語の単相化 | 各所 | それぞれ約 1% |
| `replay` のバックグラウンド圧縮、`batch` の大きい順ディスパッチ | `Crates/Cli` | 密なスナップショットの replay が 1.8 倍。バッチ末尾で 1 本だけ残ることがない |

## 現在の時間の使われ方

Callgrind、包括、最初の 5000〜7000 tick。割合は実行全体に対するもので、重なりがある。

1. `AttackExecution::tick` 44〜51%: 征服ループ (`add_neighbors` 14〜18、`GameImpl::conquer` 13〜15 のうち `update_borders` 9〜10、ヒープ 3、`DetMath` の log と exp を含む `attack_logic` 5〜6、防衛拠点クエリ 2〜4)。
2. 海路 13〜15% (C081、C023): 海岸補正、視線スムージング (`los_smooth` 4)、境界付きローカル A* (3〜4)、階層 A* (3)。
3. `PlayerExecution` のクラスタ 12〜15% (`calculate_clusters`、V36 のフラッドとピンチ処理)。
4. タイル集合と境界集合の `OrderedSet` ハッシュ操作が約 8% (`insert`、`remove`、`maybe_compact`): 専用の開番地法 `u32` 集合、または `tiles` と `border_tiles` 用のタイルごとの位置配列で大半を除ける。
5. 軍艦と交易船の tick 8〜11% (`hunt_down_trade_ship`、`find_best_target`)、列車駅 6〜8%、鉄道 `connect_station` 4〜7%。
6. seedrandom (ARC4) ビルドの `PseudoRandom::next` が C049 で 5%。

順序に触れるため注意が必要な案: `tiles` と `border_tiles` をタイルごとの位置配列で持つ、攻撃ごとに `large_territory_bonus` をキャッシュする、計算のたびに作り直すのではなくクラスタを差分更新する。

## バッチ

`ofsim batch --jobs N` はレコードをサイズの大きい順に並べ、各ワーカーが共有カウンタから次のレコードを取るため、長いゲームが最後に始まることはない。ピークメモリはジョブごと (ベンチマークのレコードで約 250〜650 MB、所有タイル数に応じて増える) なので、4 ジョブで約 2.5 GB 必要。

4 コア VM でのスケーリング (`Tools/Bench.py --scale`、ベンチマークの 7 レコードに対する `batch --verify-only`、作業量は約 136 秒):

| `--jobs` | 実時間 | tick/s (全ジョブ合計) | 高速化 | ジョブプロセス全体のピーク RSS |
| --- | --- | --- | --- | --- |
| 1 | 136.2 秒 | 499 | 1.0x | 678 MB |
| 2 | 67.9 秒 | 1,002 | 2.0x | 968 MB |
| 4 | 53.8 秒 | 1,265 | 2.5x | 1,554 MB |

長さの異なる 7 レコードでは 4 ワーカーを埋められず (理想は 36 秒)、VM 自体も線形にはスケールしない: 同じシングルスレッド実行を 4 本同時に走らせると 9.7 秒ではなく 13.9 秒かかる (4 プロセスで約 2.8 倍)。メモリ帯域とキャッシュを共有するためである。レコードが多ければ `--jobs 4` でここでは約 2.8 倍になる。ジョブ間に共有状態はない。並列スケーリングをさらに改善するには、tick あたりのメモリトラフィックを減らす (タイルセットを小さくする、上の一覧を参照) ことが効く。
