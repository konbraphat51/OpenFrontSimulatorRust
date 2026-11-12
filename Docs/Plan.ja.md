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
| P5 | `ofsim replay` のスナップショット、`batch`、プロファイリング | サンプルゲームでアップストリームのダンパーと出力が同一。スループットを計測済み | |
| P6 | 2025-10-01 (v0.25.21) から最新までの全ビルドのビルドルール。我々のデータに沿って新しい方から、その後過去へ | 各ビルドがサンプルしたレコードで通る | |
