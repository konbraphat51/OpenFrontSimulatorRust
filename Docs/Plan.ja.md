# 実装計画

English version: [Plan.md](Plan.md)

このファイルの責務: 移植のフェーズ、各フェーズの成果物、その状況。フェーズが完了したら状況列を更新する。

最初のマイルストーンはベースラインビルド v0.34.22 (`4e837520e886`) のハッシュ完全一致での再生である。以降のビルドはすべてそれとの差分である。

| フェーズ | 成果物 | 完了条件 | 状況 |
| --- | --- | --- | --- |
| P0 | ワークスペース、ドキュメント、ライセンス、サブモジュール | `cargo build` が通る | 完了 |
| P1a | `Compat`、`Core` (PseudoRandom、DetMath、Util)、`GameMap`、`TileSet`、ベースラインの `Configuration` | 単体テストが node で計算した値と一致する | 完了 |
| P1b | `ofsim-record`、`ofsim-map` | ベースラインのレコードとマップが読み込め、ターンが `decompressGameRecord` と同様に展開される | 完了 |
| P1c | リファレンストレーサーとレコード取得ツール | アップストリームエンジンから書かれたベースラインレコードのトレースがある | 完了 |
| P2 | ゲームコア: `GameImpl`、`PlayerImpl`、`UnitImpl`、攻撃、同盟、`Executor`、`GameRunner`、スポーン、部族、攻撃、兵力成長、勝利判定 | ベースラインのレコードでスポーンフェーズ終了までハッシュが一致し、その後最初のユニット建設まで一致する | コアは実装済み (ゲーム状態、スポーン、プレイヤーの維持処理、勝利判定、`ofsim verify`/`trace`)。攻撃・同盟・部族・国家は `NotPorted` のため、国家なしのゲームではスポーンフェーズまで一致する |
| P3 | ユニット: 建設、構造物、港と交易船、輸送船、軍艦、鉄道と列車、核、MIRV、SAM、砲弾、水核、経路探索 | ネーションのないベースラインレコードがすべて `ofsim verify` を通る | |
| P4 | ネーション AI (`NationExecution` と各 behaviour、`NationCreation`) | ベースラインレコードがすべて通る | `NationExecution`、emoji・alliance・warship の behaviour、`NationUtils` は移植済み (不活性トレース一致、乱数消費も一致)。structure・nuke・MIRV の behaviour (N2) と `AiAttackBehavior` (A2) は未了 |
| P5 | `ofsim replay` のスナップショット、`batch`、プロファイリング | サンプルゲームでアップストリームのダンパーと出力が同一。スループットを計測済み | 完了 (3 レコードでバイト単位一致、[Output.ja.md](Output.ja.md) 参照。バッチは並列・再開可能。スループットは下表) |
| P6 | 2025-10-01 (v0.25.21) から最新までの全ビルドのビルドルール。我々のデータに沿って新しい方から、その後過去へ | 各ビルドがサンプルしたレコードで通る | 進行中: フレームワーク (`Builds/Rules.rs`、`Builds/Maps.rs`) と世代 D のクラス C001 から C012 は完了 (下記)。C013 以降と C088 以降は未着手。C081 から C087 (モノレポ構成) は完了 |

## スループット (P5)

シングルスレッド、リリースビルド、他のジョブと共有の 4 コアマシン (負荷約 3) で `ofsim verify`。アップストリームは同じマシンでベースラインのワークツリー上の `ReplayDumper.ts` (Node 22、`--every 1000000`)。現在のホットスポットは海路探索 (階層 A*)、領土クラスタ (`PlayerExecution`)、鉄道クラスタで、いずれも移植したアルゴリズム自体です。10 倍の目標は未達で、約 3.3〜3.7 倍でした。

| レコード | tick 数 | Rust 秒 | Rust tick/s | アップストリーム tick/s | 高速化 |
| --- | --- | --- | --- | --- | --- |
| cQtf3wZ8VA | 8,503 | 34.1 | 250 | 75 (114 秒) | 3.3x |
| caYteuCaea | 29,013 | 35.4 | 820 | 226 (128 秒) | 3.6x |
| Q94sNurv | 58,511 | 34.6 | 1,690 | 450 (130 秒、100 tick ごとのスナップショット) | 3.8x |

エンジンコードへの高速化変更 (動作は不変。ベースライン 41 件中 40 件が引き続き通り、`c6dYiNx281` は 3810 で止まるまま): 鉄道 A* は照会ごとに 10 MB を確保せず、スタンプ付きバッファを再利用。`RailNetwork::find_station` はユニットごとの索引を使用。

## P6 クラス別状況

クラス ID、親、ビルドは [BuildPlan.ja.md](BuildPlan.ja.md) (状態列) と `Crates/Engine/src/Builds/BuildClasses.toml` にある。`Rules.rs` にそのクラスとその祖先の辺があれば対応済みで、それ以外のクラスは `build <commit> (class Cnnn) not ported yet` で失敗する。「一致」はサンプルレコード (`Cache/Records/<commit12>/`、最大約 12,900 ターン) を全ハッシュ一致で再生できた件数。

| クラス | ビルド | `Change` フラグの背後にある変更 | 一致 |
| --- | --- | --- | --- |
| C000 | v0.34.19 から v0.34.23 | ベースライン | 41件中40件 (c6dYiNx281 未解決) |
| C001 | v0.34.24、v0.34.25 | PR 5823 征服対象のない攻撃は `init` で撤退 | 44/44 |
| C002 から C005 | v0.34.18、v0.34.17、v0.34.16、v0.34.13 | なし (ワイヤスキーマ、統計、表示メッセージのみ) | 7/7、9/9、6/6、6/6 |
| C006 | v0.34.12 | PR 5564 (Channel Islands の部族名。マップファイルが異なり git から読む) | 8/8 |
| C007 | v0.34.11 | PR 5545 `PlayerExecution` の穴クラスタ判定、PR 5587 飽和の中点 (交易 230、列車 500) | 11/11 |
| C008 | v0.34.8 から v0.34.10 | PR 5525 味方ユニットに対する核のキル判定 | 13/13 |
| C009 | v0.34.7 | なし | 6/6 |
| C010 | v0.34.3、v0.34.4 | PR 5462 列車飽和の中点 300 | 18/18 |
| C011 | v0.34.0、v0.34.1 | PR 5425 攻撃速度の除数 7.77、PR 5436 チーム数の下限 2 なし | 12/12 |
| C012 | v0.34.14 | Canary Islands、New Zealand、Pulicat Lake の部族テーマ | 6/6 |
| C090 | v0.34.5 | なし (データテーブルとマップのマニフェストのみ。マニフェストは git から読む) | 7 / 7 |
| C081 | v0.35.0 | PR 5899 のネーションの全面改修 (`AiAttackBehavior` の報復、王冠、舟ルート、橋頭堡、ボット併合。核の追撃と MIRV 漏れ推定。構造物。軍艦による航路掃討。絵文字と同盟の調整。攻撃の関係値規則)、PR 5907 プレイヤー単位の貿易・列車スポーン加速、PR 5833 絵文字ウィンドウ、PR 5805 水グラフの辺をノード対で整列、PR 5719 列車の分割線路再調整、PR 5955 `PlayerExecution` の降灰分断地併合、PR 5919 全禁輸が同盟国を除外、ランク FFA とクラン戦の勝利判定 | 12 / 12 (Medium と Easy の長いレコード。Hard と Impossible の経路は未検証) |
| C082 | v0.35.2 | なし (マップのマニフェストのみ) | 32 / 32 |
| C083 | v0.35.3, v0.35.4 | PR 6087 (部族が同盟中の裏切り者を選ぶ、同盟更新の数え方)、PR 6089 攻撃撤退の順序、PR 6016 水核のミニタイル。ネーション AI のクラス分割は挙動を変えない | 20 / 20 |
| C084 | v0.35.5, v0.35.6 | なし (マップのマニフェストのみ) | 16 / 16 |
| C085 | v0.35.7 | チュートリアルモード (`tutorial` 設定: ネーションと部族は空き地にのみ拡大、勝利判定) | 8 / 8 (チュートリアルのレコードは 33〜44 ターン) |
| C086 | v0.35.9 | チュートリアルのサイロ、崩壊、軍艦 (`TutorialNation.rs`)、初期兵力 60,000、小型の原爆 | 24 / 24 |
| C087 | v0.35.11, v0.35.12, v0.35.14 | PR 6259 チーム割り当てで友人グループを連続して着席 (`NationCount` の選択は挙動を変えない) | 36 / 36 |
| C088 | v0.36.0 | 未移植: `PlayerExecution` の書き直し (ピンチパス、領土面積による順位付け)、`RailNetworkImpl`、MIRV と核の変更 | tick 210 で不一致 |
| C089 | v0.36.1 | 未移植 (C088 が前提) | |

注: `AiAttackBehavior` のランダムな船の目標の修正 (D1。ネーションの船の目標は `owner.isFriendly(nation)` を使う) はマージ済みで、上の件数はこれを含む。C081 から C087 (v0.35.x、ファイルは `packages/*/src/`) はネーション改修のルール集合 `NationsLikeHumans5899` を共有し、`AiAttackBehavior.rs`、`NationNukeBehavior.rs`、`NationStructureBehavior.rs`、`NationMIRVBehavior.rs`、`NationWarshipBehavior.rs`、`NationEmojiBehavior.rs`、`NationAllianceBehavior.rs` の中で分岐する (別ファイルの派生ではなくその場で)。新しいモジュール: `Nation/BotAnnexation.rs`、`Execution/SeveredLand.rs` (降灰で分断された土地の併合)、`Nation/TutorialNation.rs`。デバッグ補助: `Reference/RngLog.ts` と `OFSIM_RNG_LOG`、`OFSIM_RNG_STACK_AT` (乱数の呼び出しごとのログと、ある呼び出しのスタック)、`Reference/ExecLog.ts` (アップストリームが tick ごとに追加した実行)、`ofsim trace` の `OFSIM_UNIT_LOG=<smallId>`。C088 には新しい `PlayerExecution` のアルゴリズム (ピンチパス、`floodTerritoryAreas`、`executeSecondaryPass`、書き直された `removeCluster`)、書き直された `RailNetworkImpl`、MIRV、核、港、SAM の実行の変更が必要で、公開ゲームのレコードは tick 210 で最初に食い違う。
