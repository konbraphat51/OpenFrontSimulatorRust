# OfSimRust

OpenFront のゲームレコード用の非公式リプレイシミュレーター。

**OpenFront Inc. とは無関係であり、承認も受けていない。** © OpenFront and Contributors (移植したゲームロジック)、Rust 移植部分 © OfSimRust contributors。

English version: [README.md](README.md)

[OpenFront](https://github.com/openfrontio/OpenFrontIO) のゲームシミュレーションを Rust で再実装した、高速かつ決定的なシミュレーター。公開 OpenFront API のゲームレコードを受け取り、ゲームを tick ごとに再生して、そのゲームを実行したアップストリームのエンジンビルドと同一のゲーム状態を再現する。再現性はレコードに保存された状態ハッシュで検証される。研究やツール用途で、任意の tick の領土、兵力、ユニット、攻撃、同盟を復元できる。

状態: 開発中。対応ビルド: ベースライン v0.34.19 から v0.34.23 (クラス C000) と、クラス C001 から C012 の v0.34 系ビルド (v0.34.0 から v0.34.18、v0.34.24、v0.34.25)。それ以外のビルドは `build <commit> (class Cnnn) not ported yet` で失敗する。クラス別の状況は [Docs/Plan.ja.md](Docs/Plan.ja.md) と [Docs/BuildPlan.ja.md](Docs/BuildPlan.ja.md) を参照。

## セットアップ

```sh
git clone https://github.com/konbraphat51/OfSimRust
cd OfSimRust
cargo build --release
```

マップファイルは同梱しない。アップストリームのリポジトリを一度 clone すると、`ofsim` が各レコードのビルドのマップファイルをそこから読む。

```sh
git clone https://github.com/openfrontio/OpenFrontIO Cache/OpenFrontIO
```

## 使い方

```sh
# レコードを保存されたハッシュに対して検証する。--maps はベースラインビルド (v0.34.22) のマップディレクトリ。
# マップファイルが異なるビルドでは、ofsim が --upstream (既定 Cache/OpenFrontIO) から git で読み、
# Cache/Maps/<commit12>/ にキャッシュする。
cargo run --release --bin ofsim -- verify --record game.json.gz --maps <OpenFrontIO>/resources/maps

# 再生して 100 tick ごとにスナップショットを書く
cargo run --release --bin ofsim -- replay --record game.json.gz --maps <OpenFrontIO>/resources/maps --out Out/game --every 100
```

## 開発

- チェック: `cargo fmt --all && cargo clippy --all-targets && cargo test && Tools/CheckHeaders.sh`
- 依存クレートのライセンス: `cargo deny check licenses` (cargo-deny が必要。設定は `deny.toml`)
- 設計と移植ルール: [Docs/Architecture.ja.md](Docs/Architecture.ja.md)
- アップストリームエンジンとの差分デバッグ (`Reference/Setup.sh`、`TraceDumper.ts`、`TraceDiff.ts`) とレコード取得 (`Tools/FetchRecords.py`): [Reference/README.ja.md](Reference/README.ja.md)

## ライセンス

AGPL-3.0-only。エンジンが AGPL-3.0 の OpenFrontIO ソースの移植であるため。アップストリームの追加条項 (Section 7) が適用される: 「© OpenFront and Contributors」の表示を保持し、本プロジェクトは非公式の移植であり、「OpenFront」を名称に使わず、承認を示唆しない。マップファイルとゲームレコードは含まない。アップストリームのマップのコピーには、そのライセンスが適用される。

[LICENSE](LICENSE)、[NOTICE](NOTICE)、[Docs/Licensing.ja.md](Docs/Licensing.ja.md) を参照。法的助言ではない。
