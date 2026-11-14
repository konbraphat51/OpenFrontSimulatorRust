# 性能

English version: [Performance.md](Performance.md)

このファイルの責務: スループットの測定方法、現在のベンチマーク表、P6-P と P7 の最適化パスの変更内容、残りの課題。すべての変更に共通の不変条件は、状態ハッシュが一切変わらないこと (反復順序と浮動小数点演算の順序を保つ)。`Tools/QuickRegression.sh` で確認し、公開前に移植済み全クラスで `Tools/VerifyClass.sh` を実行する。

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
- 共有 VM のため実行時間は実行ごとに 5〜10% 変動する (他のジョブが動いているともっと大きい)。2 つのバイナリの A/B 計測は交互に実行し、ユーザー時間とシステム時間の合計 CPU 時間 (`resource.getrusage(RUSAGE_CHILDREN)`) で比べると、他のジョブの影響をずっと受けにくい。変更の前後比較には決定的な命令数を使う: `valgrind --tool=callgrind --callgrind-out-file=cg.out target/release/ofsim verify --record R --maps M --until 3000` の後 `callgrind_annotate cg.out` (自己コスト) または `--inclusive=yes`。release プロファイルは行テーブルを残しているので `--auto=yes` で熱い行が見える。
- `Tools/BuildPgo.sh` はプロファイル最適化ビルドを `target/pgo/release/ofsim` に作る (`rustup component add llvm-tools` が必要)。C081 で実行により 0〜8% 高速化 (VM が不安定)、ハッシュは変わらない。`-C target-cpu=x86-64-v3` は計測したが効果がなかった (分岐とメモリで律速) ため使わない。

## ベンチマーク (release プロファイル、1 スレッド、4 コア共有 VM)

P7 パス。レコード 1 件につき `ofsim verify` を 1 プロセス実行。「前」は統合ブランチ `b53d2ee` (P6-P パスの終わり)、「後」は P7 ブランチ。2 つのバイナリを交互に実行した 2 回の最速値で、速度は CPU 秒 (ユーザー時間 + システム時間) あたりの tick 数。ピーク RSS はそのプロセスのもの。

| クラス | レコード | tick 数 | 前 (tick/s) | 後 | 高速化 | replay 前 (実時間 tick/s) | replay 後 | ピーク RSS 前から後 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| C000 | dqD3tRRz64 | 11,102 | 633 | 771 | 1.22x | 621 | 716 | 350 から 214 MB |
| C023 | tEJEgWSc | 12,932 | 484 | 631 | 1.30x | 446 | 595 | 627 から 273 MB |
| C040 | HQ4dbm2d | 4,362 | 656 | 792 | 1.21x | 616 | 782 | 251 から 160 MB |
| C049 | FMHzJ8XF | 12,875 | 1,022 | 1,290 | 1.26x | 967 | 1,293 | 318 から 144 MB |
| C059 | 67986TJw | 7,002 | 435 | 515 | 1.18x | 424 | 505 | 375 から 276 MB |
| C081 | cjPSE6enhb | 10,712 | 762 | 986 | 1.29x | 731 | 971 | 283 から 138 MB |
| C088 | cxzGcV5tJm | 9,022 | 464 | 555 | 1.20x | 437 | 538 | 360 から 224 MB |
| 全体 | | 68,007 | 601 | 747 | 1.24x | 571 | 720 | |

命令数 (callgrind、最初の tick): C081 22.79 G から 19.79 G (3000 tick)、C059 36.54 G から 30.74 G (3000 tick)、C088 39.25 G から 34.80 G (3500 tick)。CPU 時間の改善 (1.18〜1.30 倍) が命令数の改善 (13〜16%) より大きいのは、プレイヤーのタイルセットがハッシュを使わなくなり、タイルごとの位置配列がタイル状態の近くで読まれるためキャッシュミスが減ったから。replay の出力は以前のバイナリとバイト単位で一致する (`Tools/CompareReplayOutput.py` を HQ4dbm2d、67986TJw、cjPSE6enhb、cxzGcV5tJm で実行: Result.json、Entities.ndjson、すべてのタイルフレーム)。

ジョブあたりのメモリ: ベンチマークのレコードでの replay 1 本のピーク RSS は 138〜276 MB (以前は 251〜627 MB) になり、`ofsim batch --jobs 4` に必要なのは約 2.5 GB から約 1 GB になった。節約の大半は、タイルセットと境界セットのハッシュテーブル (プレイヤーごとに `HashMap` 2 つ、それぞれそのプレイヤーのピークに合わせた大きさ) が、マップごとの `u32` 配列 2 本に置き換わったことによる。

### P6-P パス (参考)

統合ブランチ `aa545d3` との比較:

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

最初の 3000〜5000 tick の命令数は 25〜40% 減った (C081 31.5 G から 22.8 G、C000 39.6 G から 28.9 G、C088 は 5000 tick で 86.1 G から 54.2 G)。実時間の減りが小さいのは、残りの時間がタイルセットとタイル配列のキャッシュミスに支配されるため (P7 パスがここを攻めた)。以前のアップストリーム比 (別のレコードで 3.3〜3.8 倍) と合わせると、P6-P のエンジンはシングルスレッドでアップストリームの約 4.3〜5 倍だった (アップストリームは再計測していないので、P7 はその上に 1.24 倍を加える)。

スナップショット: `ofsim replay` はタイルフレームの圧縮と書き込みを 3 本のバックグラウンドスレッドで行う (`batch` ではジョブごとに 1 本)。HQ4dbm2d を `--every 1` で処理した場合、59.8 秒から 33.9 秒になった。残りは zstd レベル 3 そのもの (フレームはアップストリームのダンパーとバイト単位で一致させる必要があるため、レベルは変えられない)。

## P7 パスでの変更

| 変更 | 場所 | 効果 |
| --- | --- | --- |
| プレイヤーの `tiles` と `border_tiles` 集合をタイルごとの位置配列で持つ: 同じ系統の集合はマップを分割するので、プレイヤーごとのハッシュ索引の代わりに系統ごとに `u32` のスロット配列を 1 本持つ。配列は `TilePositions` で共有し、集合自身のスロット配列と照合するので、他のプレイヤーが残した古い値があっても無害 | `Game/TileSet.rs`、`GameImpl.rs`、`PlayerImpl.rs` | `conquer` と境界更新でハッシュなし。ピーク RSS -35〜-56%。命令数で約 3%、時間ではそれ以上 |
| `OrderedU32Set`: `SlotIndex` 上の挿入順 `u32` 集合 (後方シフト削除の開番地法テーブル、または位置配列)。カーソルと圧縮の意味は `OrderedSet` と同じで、ランダムな出し入れで `OrderedSet` と照合するテスト付き | `Compat/OrderedU32Set.rs`、`Game/AttackImpl.rs` | 攻撃の境界集合: `HashMap` 索引の約半分の命令数 |
| 大領土ボーナスのシグモイドをタイル数ごとにメモ化 (タイル数の純関数) | `Configuration/Config/Combat.rs` | `attack_logic` の `DetMath` log と exp (約 3%) がウォームアップ後は消える |
| libm なしの `floor`/`ceil` (ベースラインの x86-64 ターゲットには `roundsd` がない)、ユニットグリッドの `fmod` を厳密な整数剰余に | `Compat/JsNumber.rs`、`Game/UnitGrid.rs`、`Core/PseudoRandom.rs` | 約 2% (征服タイルごとの防衛拠点クエリ、すべての `next_int`) |
| 征服ループ: 地形を読む前に所有者を判定、境界更新で列の計算を 1 回に (`neighbors4_with_x`、`is_border_at`) | `Execution/AttackExecution.rs`、`Game/GameMap.rs`、`Game/GameImpl.rs` | 約 2% |
| サイズが正確な `TileSet::iter` (`collect` の割り当てが 1 回) | `Compat/OrderedU32Set.rs` | 境界タイルのコピー時の再割り当て減 |
| 海路スムージングの Bresenham 走査をタイル番号だけで追跡 | `Pathfinding/Transformers/SmoothingWaterTransformer.rs` | C023 で約 0.6% |
| 鉄道 A* のヒューリスティックを除算なしの `x`/`y` に、境界付き海路 A* を 32 ビット除算と libm なしの `floor` に | `Pathfinding/Algorithms/` | 展開ノードあたりのサイクル減 (報告に値する命令数の変化はない。遅いのは 64 ビット除算) |
| ARC4: 6 出力バイトを整数で累積 (2^48 未満なので厳密)、`u8` 添字 | `Core/Arc4.rs` | seedrandom ビルドの `PseudoRandom::next` で約 2% |
| Legacy28 `SerialAStar`: `gScore` と `cameFrom` を検索専用マップ 1 つに (緩和ごとのハッシュ操作が 1 回)、隣接ノードは展開ごとの `Vec` ではなく再利用バッファに書く | `Pathfinding/Legacy28/SerialAStar.rs`、`MiniAStar.rs`、`Game/RailNetwork.rs` | C059 で命令数 -10% |

試したが採用しなかったもの:

- 完成した海路を (始点、終点、水の版) でキャッシュする案: C081 と C023 でクエリの 7% しか繰り返さず (船は別々のタイルから出発する)、無効化のリスク (HPA* グラフは辺の経路をキャッシュし、再構築で入れ替わる) に見合わない。
- `flood_fill_with_gen` の内部タイル用高速経路 (境界チェックなし、隣接座標既知): 命令数が 0.5% 増えた。
- `PlayerExecution` のクラスタ差分維持: 試していない。V36 の領土フラッドと境界クラスタは先行パスが残すスタンプに依存するため、全パスを導き直さない限り、スタンプと順序が同一になることを示せない。

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

Callgrind、包括、P7 パス後の最初の 3000〜5000 tick。割合は実行全体に対するもので、重なりがある。

1. `AttackExecution::tick` 48〜56% (C081、C023、C088): `add_neighbors` 16〜19 (隣接走査、`PseudoRandom` の抽選、攻撃の境界集合、ヒープ `enqueue`)、`GameImpl::conquer` 14〜18 のうち `update_borders` 10〜12 (征服タイルごとに `is_border_at` を 5 回評価)、ヒープ `dequeue` 4〜5 (優先度の同値でアップストリームの正確なシフト順が必要なのでヒープは差し替えられない)、`attack_logic` 3〜4、`refresh_to_conquer` 4〜5 (全境界タイルの再投入走査。抽選が順序依存)。
2. `PlayerExecution` のクラスタ: C081 で 14%、C088 で 31%。C088 (v0.36 以降) では `remove_clusters_v36` が 22%: スキャンライン領土フラッドが約 18% (タイルごとに訪問スタンプと所有者判定、メモリ律速)、`calculate_clusters` が 8%。これ以上は別のアルゴリズム (または差分更新でスタンプと順序が保たれることの証明) が必要。
3. 海路: C023 で 15%、C081 で 8%、C088 で 6%: 階層 A* 4、境界付きローカル A* 3〜4、C023 の `los_smooth` 5 (二分探索が長い線上で `can_see` を何度も呼ぶ)、交易船の `find_path` は 1 回あたり約 120 万命令。
4. C023 の交易船 10% と軍艦 7% (`hunt_down_trade_ship`、`find_best_target`、`can_attack_player`)、C081 の列車駅 tick と `connect_station` で約 6%。
5. Legacy28 ビルド (C059): 双方向 A* が実行の 43% (`FastPriorityQueue::poll` 8、ハッシュマップ 5、`expand_node` 11)、ARC4 の抽選が約 4%。
6. 攻撃の境界集合 (ハッシュ索引の `OrderedU32Set`) 約 4%: 攻撃間でタイルが排他的ではないので、共有の位置配列は使えない。

## バッチ

`ofsim batch --jobs N` はレコードをサイズの大きい順に並べ、各ワーカーが共有カウンタから次のレコードを取るため、長いゲームが最後に始まることはない。ピークメモリはジョブごと (ベンチマークのレコードで約 140〜280 MB、所有タイル数に応じて増える。P7 パス前は 250〜630 MB) なので、4 ジョブで約 1 GB 必要。

P7 パス前の 4 コア VM でのスケーリング (`Tools/Bench.py --scale`、ベンチマークの 7 レコードに対する `batch --verify-only`、作業量は約 136 秒):

| `--jobs` | 実時間 | tick/s (全ジョブ合計) | 高速化 | ジョブプロセス全体のピーク RSS |
| --- | --- | --- | --- | --- |
| 1 | 136.2 秒 | 499 | 1.0x | 678 MB |
| 2 | 67.9 秒 | 1,002 | 2.0x | 968 MB |
| 4 | 53.8 秒 | 1,265 | 2.5x | 1,554 MB |

長さの異なる 7 レコードでは 4 ワーカーを埋められず (理想は 36 秒)、VM 自体も線形にはスケールしない: 同じシングルスレッド実行を 4 本同時に走らせると 9.7 秒ではなく 13.9 秒かかる (4 プロセスで約 2.8 倍)。メモリ帯域とキャッシュを共有するためである。レコードが多ければ `--jobs 4` でここでは約 2.8 倍になる。ジョブ間に共有状態はない。並列スケーリングをさらに改善するには、tick あたりのメモリトラフィックを減らす (タイルセットを小さくする、上の一覧を参照) ことが効く。
