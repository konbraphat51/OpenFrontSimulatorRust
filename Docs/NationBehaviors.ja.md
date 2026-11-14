# ネイション行動 (N2): 構造物、核、MIRV

English version: [NationBehaviors.md](NationBehaviors.md)

このファイルの責務: `Crates/Engine/src/Execution/Nation/` にある 3 つのネイション行動の移植内容、`NationExecution` との接続、検証方法を説明する。

## ファイル

| Rust ファイル | 上流 | `NationExecution` からの呼び出し |
| --- | --- | --- |
| `NationStructureBehavior.rs` | `execution/nation/NationStructureBehavior.ts` | `handle_structures(game) -> bool` |
| `NationNukeBehavior.rs` | `execution/nation/NationNukeBehavior.ts` | `maybe_send_nuke(game)`, `find_best_nuke_target(game)` |
| `NationMIRVBehavior.rs` | `execution/nation/NationMIRVBehavior.ts` | `consider_mirv(game) -> bool` |
| `NationBehaviorPorts.rs` | (なし) | トレイトと決定ログ (下記) |

3 つとも、ネイションが共有する `SharedRandom` (上流は 1 つの `PseudoRandom` を全行動に渡すため乱数の消費が交互になる) と、コンストラクタの `&GameImpl`、各メソッドの `&mut GameImpl` を受け取る (N1 の `NationApi` を参照)。

## 設計上の要点

- **価値関数** (構造物行動の `SpawnValue`) は、上流のクロージャと同じく配置探索ごとに 1 回作る。作成時に乱数を消費することがある (`shouldUseConnectivityScore`、SAM のカバレッジ抽選) ため、価値関数の作成とタイル抽出の順序も契約の一部である。
- **`recentMirvTargets`** は上流クラスの static。ここでは `GameImpl::recent_mirv_targets` に置き、同一プロセス内の複数ゲームが互いに影響しないようにした。
- **協調相手** は `Rc<RefCell<dyn EmojiPort>>` と `Rc<RefCell<dyn AttackPort>>` で保持する (`NationBehaviorPorts.rs`)。`NationEmojiBehavior` (N1) と `AiAttackBehavior` (A2) の実装はそこにあり、N1 は自分の `Shared<..>` を渡せば Rust が変換する。テストではダブルを渡す。
- **生成される Execution** (`ConstructionExecution`、`UpgradeStructureExecution`、`NukeExecution`、`MIRVExecution`) は A3 と A4 の担当。`DecisionLog` で行動をドライランにでき (`behaviour.log.set_dry_run(true)`)、Execution を積まずに決定を記録する。ゴールデンテストはこれで観測している。
- **鉄道網**: 接続性スコアは `RailNetwork::station_clusters()` (ユニット、クラスタ ID) を読む。スタブは空を返す (駅がまだ存在し得ないため)。A3 の `RailNetworkImpl` は `stationManager().getAll()` の順にすべての駅を返し、同じクラスタの駅には同じ ID、孤立駅には `None` を付けること。
- `GameImpl::shared_water_components(player)` が `SharedWaterCache` を保持し、`SharedWaterView` を実装する。

## 検証

`Crates/Engine/tests/NationBehaviorGoldenTest.rs` は `tests/Golden/NationBehaviors/<name>.spec.json` (プレイヤー、占領した正方形、構造物、関係、攻撃、tick、行動呼び出しからなるシナリオ) を Rust エンジンで実行し、各ステップを `<name>.golden.ndjson` と比較する。ゴールデンは `Reference/NationBehaviorDump.ts` で上流エンジンから作る。比較するのは戻り値、積まれた Execution (種別、タイルまたはユニット、待機 tick)、ゲーム状態ハッシュ。ローカルキャッシュ (`Cache/Worktrees/4e837520e886/resources/maps`) が無い場合はスキップされる。

ゴールデンの再生成 (ベースラインの worktree で、`Reference/*.ts` を `ofsim-reference-n2/` にコピーしてから):

```sh
npx tsx ofsim-reference-n2/NationBehaviorDump.ts <spec.json> <name>.golden.ndjson
```

シナリオは 4 つの難易度、FFA とチーム戦、高い初期ゴールド、陸と船の攻撃を受けた場合 (防衛施設)、密集した領土 (アップグレード)、王冠と最強チームの標的、SAM の軌道、SAM に対する原爆の一斉発射、MIRV の反撃、勝利阻止と圧勝阻止の規則を網羅する。
