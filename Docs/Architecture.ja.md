# アーキテクチャ

English version: [Architecture.md](Architecture.md)

このファイルの責務: シミュレーターの設計と、すべての貢献者 (人間もエージェントも) が従う移植ルール。エンジンのコードを書く前に読むこと。

## 目標

2025-10-01 以降の公開 OpenFront ゲームレコードを tick ごとに再生し、ゲーム状態をレコードの `gitCommit` のアップストリーム TypeScript エンジンと**ビット単位で同一**にし、かつアップストリームエンジンよりはるかに高速に動かす。正しさは、アップストリームエンジンが 10 ターンごとにすべてのレコードへ書き込む状態ハッシュ (`GameImpl.hash()`) と、デバッグ用のアップストリームエンジンとの差分トレースで担保する。

## ワークスペース

| クレート | パス | 責務 |
| --- | --- | --- |
| `ofsim-record` | `Crates/Record` | レコード JSON スキーマ (serde)、gzip 読み込み、疎なターンの展開 (`decompressGameRecord`)、エンジンが消費する開始情報 (`toWireGameStartInfo`)。エンジンロジックは持たない。 |
| `ofsim-map` | `Crates/Map` | マップディレクトリ (`map.bin`、`map4x.bin`、`map16x.bin`、`manifest.json`) を素のデータとして読むことと、`GameMapType` 名からディレクトリへの対応表。エンジンロジックは持たない。 |
| `ofsim-engine` | `Crates/Engine` | 決定的シミュレーション: ゲーム状態、execution、経路探索、AI、ハッシュ、ビルドルール。 |
| `ofsim` | `Crates/Cli` | `ofsim` CLI: `verify`、`replay` (スナップショット)、`trace`、`trace-diff`、`batch`。 |
| リファレンストレーサー | `Reference/` | アップストリームのワークツリーにコピーして使う TypeScript スクリプト。差分デバッグのため `ofsim trace` と同じトレースを出力する。 |

## エンジンのモジュール構成

Rust のモジュールは、すべての Rust ファイルがヘッダコメントに書かれたアップストリーム上の対応ファイルを 1 つ持つように、アップストリームの `src/core/` を鏡写しにする (`//! Port of src/core/game/PlayerImpl.ts`)。

```
Crates/Engine/src/
  Lib.rs
  Compat/        JS のセマンティクス: 数値ヘルパー、挿入順の set と map、安定ソート
  Core/          PseudoRandom (sfc32 または seedrandom ARC4), DetMath, Util (simpleHash, toInt, ...)
  Configuration/ Config (そのビルドの DefaultConfig の値)、ユニット情報
  Game/          GameMap, TileSet, Game (共有の型と enum), GameImpl, PlayerImpl, UnitImpl,
                 AttackImpl, AllianceImpl, AllianceRequestImpl, UnitGrid, RailNetwork, TeamAssignment,
                 NationCreation, WaterManager, ...
  Execution/     アップストリームの execution ごとに 1 ファイル。Nation/ と Alliance/ のサブフォルダもアップストリームどおり
  Pathfinding/   アップストリームどおり
  Runner/        GameRunner, Executor (ExecutionManager.ts)
  Builds/        Rules: あるビルドがどのアップストリームの挙動を持つか
```

命名はリポジトリの規約に従う。ディレクトリとファイルは UpperCamelCase (モジュール名についてはクレート単位で `non_snake_case` を許可)、関数・メソッド・変数は Rust の snake_case、型は UpperCamelCase。モジュールと型は型名前空間を共有するため、同じ親モジュール内で同名にしてはならない。したがってモジュール `X` の親で `pub use X::X` はしない。型は完全なパスで import する (`use crate::Game::GameMap::GameMap;`)。

## コアデータモデル

アップストリームのエンジンはオブジェクトグラフである (プレイヤーがユニットを持ち、ユニットが所有者を持ち、execution がすべてを持つ)。Rust では所有者を 1 つ、`GameImpl` にして、整数ハンドルを使う。

| アップストリームのオブジェクト | Rust のハンドル | 格納先 |
| --- | --- | --- |
| `TileRef` | `TileRef = u32` | `GameMap` の配列 |
| `Player` / `TerraNullius` | `SmallId = u16` (0 = terra nullius) | `GameImpl.players: Vec<PlayerImpl>`、添字は `small_id - 1`、挿入順 (プレイヤーは削除されないので JS の `Map` の順序でもある) |
| `Unit` | `UnitId = u32` (アップストリームの `nextUnitID`) | id をキーとする `GameImpl.units`。各プレイヤーは `PlayerImpl._units` とまったく同様に自分の順序付き `Vec<UnitId>` を持つ |
| `Attack` | `AttackId` (アリーナ添字) | `GameImpl` 内のアリーナ。プレイヤーは `_incomingAttacks` のように順序付き id リストを持つ |
| `Alliance`、`AllianceRequest` | アリーナ id | `GameImpl` 内のアリーナ。アップストリームが配列を持つ箇所は順序付きリスト |

他のオブジェクトに触れる `PlayerImpl`、`UnitImpl`、`AttackImpl` のアップストリームのメソッドは、ゲームを借用する短命なハンドルに移植する。

```rust
pub struct PlayerRef<'a> { game: &'a GameImpl, id: SmallId }      // read-only methods
pub struct PlayerMut<'a> { game: &'a mut GameImpl, id: SmallId }  // mutating methods
// game.player(id).troops(), game.player_mut(id).conquer(tile)
```

メソッド名はアップストリームの名前の snake_case とし、移植したコードが原典のように読めるようにする。同じパターンをユニット (`UnitRef`/`UnitMut`) にも適用する。

## Execution

```rust
pub trait Execution {
    fn init(&mut self, game: &mut GameImpl, ticks: Tick);
    fn tick(&mut self, game: &mut GameImpl, ticks: Tick);
    fn is_active(&self) -> bool;
    fn active_during_spawn_phase(&self) -> bool;
}
```

`GameImpl` は `execs: Vec<Box<dyn Execution>>` と `un_init_execs` を保持する。`execute_next_tick` は `execs` を tick 中は `std::mem::take` で取り出すため、execution はゲームを自由に可変借用できる。その間に追加された execution はアップストリームどおり `un_init_execs` に入る。`GameImpl.executeNextTick` におけるアップストリームの処理順 (アクティブな exec の tick、未初期化 exec の init、整理、追加、player update、10 tick ごとのハッシュ、water manager、`_ticks++`) は、癖も含めて正確に再現する (たとえば、同じループ内の `init` 呼び出し中に追加された execution は、アップストリームが `unInitExecs` を再代入するため捨てられる)。アップストリームの癖を「直す」ことは決してしてはならない。直せば非同期 (desync) になる。

### ゲームと Execution の API (P2)

- `GameImpl::new(humans, nations, map, mini_map, config, team_spawn_areas)`。`GameImpl::execute_next_tick() -> Option<f64>` は 1 ティックを実行し、10 ティックごとにそのティック内で計算したハッシュ (レコードに保存される値) を返す。公開フィールドとアクセサからすべてにアクセスできる: `game.map` (`GameMapImpl`)、`game.config()`、`game.stats_mut()`、`game.ticks()`。
- プレイヤー: `game.player(id) -> PlayerRef` (読み取り) と `game.player_mut(id) -> PlayerMut` (書き込み)。どちらも単純なゲッターのために `PlayerImpl` へ deref する。`id` は small id で、`0` は terra nullius。ユニット: `game.unit(id)` (削除後は None)、`game.unit_any(id)` (削除済みも含む)、`game.unit_mut(id) -> UnitMut`。攻撃・同盟・リクエスト: `game.attack(id)`、`game.alliance(id)`、`game.alliance_request(id)` と各 `_mut`。`PlayerMut::create_attack`、`GameImpl::delete_attack`。
- Execution は `Execution { init, tick, is_active, active_during_spawn_phase }` を実装し (すべて `&mut GameImpl` を取る)、`game.add_execution(Box::new(..))` で追加する。`Runner/Executor.rs` が intent を Execution に対応付ける。Execution を移植したら、その intent の `NotPorted::at_creation("<Name>")` の分岐を置き換え、`Reference/InertPatch.ts` の `PORTED_INTENTS` に intent を追加する。`GameRunner::init` はすべてのゲームが最初に持つ Execution を作る。
- `NotPorted` の Execution は `not ported: <Name>` でパニックし、`ofsim verify` がそのティックとメッセージを報告する。本物の `WaterManager`、水上系 Execution (輸送船、帰投、交易船、軍艦、軍艦移動)、鉄道網 (駅・線路・クラスタ・列車は `Game/RailNetwork.rs` と `Game/TrainStation.rs`) は移植済み。水上系 Execution の差分シナリオは `Reference/NavalScenarioDump.ts` (`ofsim naval-scenario`、トレーサ側は `OFSIM_INERT_CLASSES=Attack,Shell` で実行)。
- クライアント専用の処理は移植しない (更新、名前配置、UI 向けクエリ、ほとんどの `stats()` 呼び出し)。シミュレーションが読み戻すのは人間が送った攻撃数と MIRV の発射数だけで、`StatsImpl` に保持する。
- デバッグ: `Reference/RngLog.ts` と `OFSIM_RNG_LOG=<ファイル>` (乱数の呼び出しごとに `<カウンタ語> <生の u32>` を出力。ストリームの生成順が異なるので、全体の行ではなくストリームごとに比較する)、`OFSIM_RNG_STACK_AT=<n>` (n 番目の呼び出しのスタック)、`Reference/RngLogArc4.ts` (seedrandom を使う era S ビルド向けの同等品: `A <最初の値> <呼び出し番号> <値>` の行と `T <tick>` マーカー。Rust 側は `OFSIM_RNG_LOG` が同じ形式を書く)、`Reference/ExecLog.ts` (アップストリームが tick ごとに追加する実行。`OFSIM_EXEC_LOG=<開始>,<終了>` は ofsim が追加する実行を出力する)、`ofsim trace` の `OFSIM_UNIT_LOG=<smallId>`、`Tools/VerifyAll.sh` (一覧表)、`Tools/TraceCompare.sh` (参照トレース、`ofsim trace`、`TraceDiff`。`--inert` で未移植の Execution を両側で無効化)、シード付きの領土シナリオ (`ofsim scenario`、`Reference/ScenarioDump.ts`)、シード付きの核・SAM・Doomsday Clock シナリオ (`ofsim nuke-scenario`、`Reference/NukeScenarioDump.ts`、`Tools/NukeScenarioCompare.sh`。毎 tick のタイル状態ダイジェストで放射性降下物と水変換も比較する)。`Crates/Engine/tests/VerifyRecordsTest.rs` は `tests/Golden/VerifyMinimums.json` の一致ハッシュ数が後退しないことを保証する。

## 決定性ルール (移植の契約)

1. **数値。** JS の `number` は `f64`。値がすべての使用箇所で Rust の型に収まる整数であると証明できる場合を除き `f64` のままにし、アップストリームが切り捨てる箇所 (`Math.floor`、`Math.trunc`、`| 0`、`toInt`) で正確に変換する。アップストリームの `bigint` (兵力、ゴールド) は範囲に応じて `i64` か `i128` にする。ビット演算子は JS の `ToInt32`/`ToUint32` のセマンティクスで再現する (`Compat::JsNumber`)。`Math.round` は `floor(x + 0.5)` であり、Rust の `round` ではない。浮動小数点演算の順序はアップストリームのまま保つ。`a * b / c` と `a * (b / c)` は異なる。
2. **超越関数。** `DetMath` を持つビルドは移植した `Core::DetMath` をビット単位で使う。それ以前のビルドは V8 の `Math.exp/log/pow/atan2/sin/cos` を呼ぶ。これらは V8 の fdlibm (`src/base/ieee754.cc`) から `Compat::V8Math` に移植し、Rust の `f64` メソッドは決して使わない (`sqrt` はどちらでも正確なので Rust のものを使ってよい)。
3. **反復順序。** JS の `Map` と `Set` は挿入順に反復し、削除して再追加すると末尾に移り、反復中に追加された要素も訪問される。順序が状態に影響しうる箇所では `Compat` の順序付きコレクション (または移植した `TileSet`) を使い、`HashMap`/`HashSet` の反復は使わない。純粋な検索用途なら `HashMap` でよい。
4. **ソート。** `Array.prototype.sort` は安定である。同じ比較関数で安定ソートを使う。比較関数が不整合になりうる場合 (NaN、何にも決められない同順位) は、同等だと仮定せず V8 の TimSort を移植する (`Compat::JsSort`)。
5. **乱数。** `PseudoRandom` のインスタンスはすべてアップストリームとまったく同じようにシード (ゲーム、executor、プレイヤー、部族スポーナー、execution ごとに `mg.ticks()` を使うなど) し、同じ順序で引く。引き漏れや余分な引きは、それ以降のすべてを非同期にする。PR 4233 より前のビルド (era S、C036 以前) は npm `seedrandom` (ARC4、`Core/Arc4.rs`) を `String(seed)` でシードして使う。生成器は必ず `PseudoRandom::for_rules(rules, seed)` で作り、`Change::Sfc32PseudoRandom4233` が sfc32 と ARC4 を切り替える。
6. **文字列。** プレイヤー id、`simpleHash` の入力、`nextID()` の出力はバイト単位で同一でなければならない (`toString(36)`、`padStart`)。`simpleHash` は UTF-16 コードユニットを反復する。
7. **クライアント専用コードは移植しない。** 描画、クライアントへ送る統計、状態に届かない絵文字やチャットは、シミュレーションのロジックが読む場合を除き移植しない (飛ばす前に確認すること。たとえばネーション AI は絵文字と関係値を読む)。
8. **エラー。** アップストリームが tick 内で throw する箇所では、ランナーがその tick のエラーを報告する。取り繕ってはならない。

## ビルドルール (バージョン)

すべてのレコードは、それを実行したサーバーのアップストリーム `gitCommit` を持つ。`Builds/Rules.rs` はコミットを `Rules` 値に対応付ける。`Rules` は、そのビルドに含まれる名前付きアップストリーム変更 (アップストリームの PR にちなんだ `Change::DetMath5191`) の集合である。リリースブランチは cherry-pick を受けるため、所属はビルドごとであり線形なバージョン順ではない。

対応付けは 2 層である。`Builds/BuildClasses.toml` (`Tools/BuildPlanGen.py` が生成しチェックイン済み。`include_str!` で埋め込み、一度だけ解析する) は、既知のすべてのビルド (コミット先頭 12 文字) をそのクラス (シミュレーションコードが同一のビルド群) に対応付け、各クラスにベースラインクラス C000 を根とする差分木での親を与える。`Rules.rs` の `EDGES` 表は、移植済みクラスが親に対して追加または削除する `Change` を定める。クラスの集合は親の集合に辺を適用したものである。辺のないクラス、または祖先に辺がないクラスは未移植で、`Rules::for_commit` は `build <commit> (class Cnnn) not ported yet` で失敗する。表にないコミットは `unsupported build <commit>` で失敗し、先頭が最も長く一致する既知ビルドを示す。

エンジンのコードは、アップストリームが変更した正確な場所で `game.config().rules().has(Change::X)` により分岐する。`has` はそのビルドが変更後の挙動を含むとき true なので、ベースライン (最新) のコードが `true` 側になり、古いビルドの辺は主に変更を取り除く。サブシステムが丸ごと書き換えられた場合 (たとえば経路探索) は、新旧の実装を 1 つのトレイトの裏に並べて置く (`AStarWaterV1.rs`、`AStarWaterV2.rs`)。 v0.28.4 のエンジン (クラス C059) は `Pathfinding/Legacy28/` (双方向 `SerialAStar`、`MiniAStar`、ステッパー)、`Execution/FakeHumanExecution.rs`、`Execution/BotExecutionV28.rs`、`Execution/Utils/*V28.rs` にあり、いずれも 1 つの `V28` ルールフラグから呼ばれる。ビルド間で変わる設定定数は `Configuration` に置き、`Rules` で選択する。ベースラインビルドは v0.34.22 (`4e837520e886`) で、他のビルドはアップストリームの `src/core` をそれと比較して追加する。

マップや名前表もビルド間で変わる。`Builds/Maps.rs` はレコードのコミットの `resources/maps/<map>` の git ツリーをベースラインのものと比較し、異なる場合は `ofsim verify/trace/replay` がアップストリームの clone (`--upstream`、既定 `Cache/OpenFrontIO`) からそのマップを一度だけ `Cache/Maps/<commit12>/` に取り出して読む。ビルド間で異なる部族名データは `Execution/Utils/TribeNames.rs` で `Change` フラグにより選択する。


v0.35.0 (C081 以降) から上流のソースは `src/core/` から `packages/{engine,engine-lib,engine-api,shared}/src/` に移り、`@openfront/<package>/...` で import される。移植ファイルのヘッダーはベースラインの `src/core/...` パスのままにする。リファレンスツールは両方の構成に対応する: `Reference/MakeCoreShim.mjs` が該当 worktree に `src/core/` の再エクスポート shim を作り、`Reference/EngineSupport.ts` の `createRunnerFor` がマップローダーの代わりに読み込み済みマップファイルを渡し、`Tools/GenEngineData.mts` が両方の構成を探す。対応: `Schemas` は `engine-api/Schemas`、`Util` は `engine-lib/Util` と `shared/SharedUtil` に分割、`Game` は `engine/game/Game` と `engine-api/game/GameTypes`、`GameMap` は `engine-api/game/GameMap` と `engine-lib/game/GameMapImpl`、`Config` は `shared/configuration` と `engine/configuration/EngineConfig`。execution、pathfinding、game の各ファイルは `engine/src/` 以下で相対パスを保つ。

## 検証

- `ofsim verify <record>` は再生し、ハッシュを持つすべてのターン (10 ターンごと) で計算したハッシュとレコードを比較する。最後のターンまですべてのハッシュが一致すればレコードは合格である。
- `Reference/TraceDumper.ts` はアップストリームエンジンを、`ofsim trace` は我々のエンジンを動かす。どちらも tick ごとに 1 行の NDJSON (ハッシュとプレイヤーごとの兵力、タイル、ゴールド、ユニットハッシュ。指定 tick では完全なタイル所有者も) を書く。`ofsim trace-diff` は最初に異なる tick と最初に異なるプレイヤーやユニットを報告する。そこが調べる場所である。
- 単体テストは、存在する場合はアップストリームのテスト (アップストリームのリポジトリの `tests/`) を移植し、`Compat` と `Core` を node で計算した値と照合する。
- 実レコードはコミットしない (ユーザー名を含むため)。それを必要とするテストはローカルのキャッシュディレクトリを読み、存在しなければスキップされる。

## 出力

`ofsim replay --every N` は OpenFrontDs プロジェクトのアップストリームベースの replay ダンパーと同じファイルを書き、利用側がエンジンを切り替えられるようにする: `Tiles/T{tick:06}.u16.zst` (zstd 圧縮のリトルエンディアン `u16` タイル状態)、`Entities.ndjson` (スナップショットごとのプレイヤー、ユニット、攻撃、同盟。人間の名前は含まない)、`Result.json` (判定、検証したハッシュ数、最初の不一致)。

## ライセンス

エンジンは AGPL-3.0 の OpenFrontIO コードの移植であるため、リポジトリ全体が AGPL-3.0-only である。マップファイルとレコードはここでは再配布しない。[Licensing.ja.md](Licensing.ja.md) と `NOTICE` を参照。

ファイルヘッダーの規約: すべての Rust と TypeScript のソースファイルは `// SPDX-License-Identifier: AGPL-3.0-only` で始まり、続けて責務を述べる `//!` コメントを置く。移植ファイルではそのコメントにアップストリームのパスを書く。例: `//! Port of src/core/game/PlayerImpl.ts (OpenFrontIO, © OpenFront and Contributors).`、または次行に `//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.` を置く。`.rs`・`.ts`・`.mts`・`.mjs` ファイルに SPDX 行がないと `Tools/CheckHeaders.sh` が失敗する。
