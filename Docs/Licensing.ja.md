# ライセンス

English version: [Licensing.md](Licensing.md)

これは法的助言ではない。このリポジトリとアップストリーム OpenFront のライセンスとの関係、およびそこから導かれると我々が考えることを説明する。アップストリームの `LICENSE`、`LICENSING.md`、`LICENSE-ASSETS`、`CREDITS.md` は各自で確認すること。

## なぜ AGPL-3.0-only か

ofsim は OpenFrontIO の `src/core` を Rust に翻訳したものであり、そのコードの派生物である。アップストリームのコードは 2025-09-04 以降 AGPL-3.0-only (著作権は OpenFront Inc. とコントリビューター) で、それ以前は MIT、続いて GPL の時期があった。このシミュレーターが対象とする全ビルド (2025-10-01 以降) は AGPL-3.0-only であり、AGPL への切り替え以前のビルドは対象外なので、リポジトリ全体を AGPL-3.0-only とする。ルートの `LICENSE` は追加条項を含むアップストリームの `LICENSE` そのものである。

## Section 7 追加条項

アップストリームは 2025-12-16 以降、AGPL の Section 7 条項を追加している。本リポジトリの対応:

| 条項 | 要求 | 対応 |
| --- | --- | --- |
| 7(b) | 「(c) OpenFront and Contributors」の表示を目につく場所に保持する | `NOTICE`、README、`ofsim --version` / `--help` の出力。移植ファイルはヘッダーにアップストリームのパスを記す |
| 7(c) | 改変版は出所を偽らず、「OpenFront」を主たる名称に使わず、公式の承認を示唆しない | 名称は `ofsim`、非公式の移植と明記し、README、`NOTICE`、CLI に非提携の表明を置く |

未決事項: リポジトリ名 `OpenFrontSimulatorRust` には「OpenFront」が含まれる。シミュレーション対象を示す記述的な言及で主たる名称ではないが、`ofsim` などへの変更をオーナーが選ぶ可能性がある。

## 利用者の義務

- 再配布 (ソースまたはバイナリ) では `LICENSE`、`NOTICE`、ファイルヘッダーを保持し、対応するソースを AGPL-3.0-only で提供する。
- Section 13: 改変した ofsim をネットワークサービスとして動かし、利用者が対話する場合、その利用者に改変版の対応ソースを提供する。
- 改変には改変した旨を明示し (Section 5(a))、「OpenFront」を主たる名称にせず、承認を示唆しない (7(c))。
- コントリビューションは AGPL-3.0-only で受け入れる (inbound = outbound)。新規ソースファイルは [Architecture.ja.md](Architecture.ja.md#ライセンス) のヘッダーを付ける。

## マップデータとアセット

このリポジトリにはマップファイル、アセット、プロプライエタリなコンテンツを含まない。マップはアップストリームのチェックアウト (`resources/maps`) から入手し、そのコピーのライセンスが利用者に適用される:

- `/resources` は CC BY-SA 4.0、表示は「OpenFront」/「OpenFront Inc.」。
- マップデータはさらに OpenStreetMap (ODbL)、Natural Earth、Bedmap3 (CC BY 4.0)、Copernicus DEM などに由来する (アップストリームの `CREDITS.md` 参照)。
- `/proprietary` は All Rights Reserved で、ofsim は読まない。

## 出力

スナップショット (`Tiles/*.u16.zst`、`Entities.ndjson`) と派生データセットは、マップデータとゲームレコードから計算される。

- 公開する場合は「OpenFront」をクレジットする。マップタイルのデータ (地形、陸水マスク、マップ上のタイル所有) を含む場合、元のマップの CC BY-SA 4.0 の継承条件や ODbL の影響が及ぶ可能性があるので、データのライセンスを決める前に確認する。
- ユーザー名は決して公開しない。公開 API のゲームレコードにはユーザー名が含まれる。このリポジトリはレコードや名前を保存も再配布もせず、`Entities.ndjson` に人間の名前は含まれない。
- ゲームレコードは公開 OpenFront API 由来で、明示的なライセンスの記載はない。再配布しないこと。

## サードパーティコード

- JavaScript と同じ結果を得るために移植した V8 / Sun fdlibm の数学関数: [ThirdPartyNotices/V8.md](../ThirdPartyNotices/V8.md) (BSD-3-Clause と fdlibm の表示)。
- Rust クレートは各自のライセンスを保つ。`deny.toml` は AGPL と互換のものだけを許可する。`cargo deny check licenses` を実行する。

## ファイルヘッダー

すべての `.rs` と `.ts` ソースファイルは `// SPDX-License-Identifier: AGPL-3.0-only` で始まり、`Tools/CheckHeaders.sh` が検査する。
