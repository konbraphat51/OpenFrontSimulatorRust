# SPDX-License-Identifier: AGPL-3.0-only
"""Prose of Docs/BuildPlan.md and Docs/BuildPlan.ja.md (English primary, Japanese secondary).

Responsibility: hold the fixed text of the P6 build plan and splice in the generated numbers
and the class table; called by BuildPlanDoc.py.
"""
from collections import Counter


def render(ja, data, builds, table):
    def t(en, jp):
        return jp if ja else en

    cl = data["classes"]
    byid = {c["id"]: c for c in cl}
    n = len(cl)
    tot = sum(c["weight"] for c in cl)
    top = sorted(cl, key=lambda c: -c["weight"])[:10]
    risk = Counter(c["risk"] for c in cl)
    era = data["era"]
    era_classes = Counter(era.values())
    era_builds = Counter()
    era_weight = Counter()
    for c in cl:
        era_builds[era[c["id"]]] += len(c["builds"])
        era_weight[era[c["id"]]] += c["weight"]
    rewrites = [(c["id"], c["parent"], x) for c in cl for x in c["changes"] if x["rewrite"]]
    size = sum(c["size_est"] for c in cl)
    ms = data["mapstats"]
    top2 = 100 * (top[0]["weight"] + top[1]["weight"]) / tot
    boundaries = [f"{c['parent']}->{c['id']} ({era[c['parent']]}->{era[c['id']]})" for c in cl if c["parent"] and era[c["parent"]] != era[c["id"]]]
    late_f = [c["id"] for c in cl if era[c["id"]] == "F" and c["last"] >= "2026-07-01"]
    prng_edges = [c["id"] for c in cl if any(x["file"] in ("PseudoRandom.ts", "DetMath.ts") for x in c["changes"])]
    top_rows = "\n".join(
        f"| {c['id']} | {', '.join('`' + b + '`' for b, _ in c['builds'][:3])} | {c['weight']} ({100 * c['weight'] / tot:.1f}%) | {era[c['id']]} | {c['risk']} | {c['lines']} |"
        for c in top)
    rew_rows = "\n".join(
        f"| {cid} | {par} | `{x['file']}` | {x['kind']} | +{x['added']}/-{x['deleted']} |" for cid, par, x in rewrites)
    gen = lambda cid: byid[cid]["first"]
    out = []
    w = out.append
    w("# " + t("Build Porting Plan (P6)", "ビルド移植計画 (P6)"))
    w("")
    w(t("Responsibility: groups the 159 deployed builds into classes of identical simulation code, orders the classes as a diff tree rooted at the baseline `4e837520e886` (v0.34.22), summarises every edge, and records the Node/V8 and hash-origin findings that bound the bit-exact goal. Machine-readable twin: `Crates/Engine/src/Builds/BuildClasses.toml`. Generators: `Tools/BuildClassify.py`, `Tools/BuildPlanGen.py`, `Tools/BuildPlanDoc.py` (prose in `Tools/BuildPlanProse.py`).",
        "責務: デプロイ済み159ビルドをシミュレーションコードが同一のクラスに分け、基準 `4e837520e886` (v0.34.22) を根とする差分木として並べ、各辺の差分を要約し、ビット一致の目標を左右するNode/V8とハッシュ生成元の調査結果を記録する。機械可読版: `Crates/Engine/src/Builds/BuildClasses.toml`。生成スクリプト: `Tools/BuildClassify.py`、`Tools/BuildPlanGen.py`、`Tools/BuildPlanDoc.py` (本文は `Tools/BuildPlanProse.py`)。"))
    w("")
    w("## " + t("Summary", "要約"))
    w("")
    w(t(f"- {n} equivalence classes for 159 builds (class hash = SHA-256 over the sorted `(path, blob)` pairs of the simulation-relevant core files). The baseline class C000 holds 5 builds (v0.34.19 to v0.34.22).",
        f"- 159ビルドは {n} 個の等価クラスに分かれる (クラスハッシュ = シミュレーション関連のcoreファイルの `(パス, blob)` をソートして取ったSHA-256)。基準クラスC000は5ビルド (v0.34.19〜v0.34.22) を含む。"))
    w(t(f"- Risk split: high {risk['high']}, medium {risk['medium']}, low {risk['low']}. Estimated total if every class were ported: about {size:,} Rust lines (added lines plus a quarter of deleted lines over all edges; code touched by several edges is counted repeatedly, so this is an upper bound).",
        f"- リスク内訳: 高 {risk['high']}、中 {risk['medium']}、低 {risk['low']}。全クラスを移植した場合の推定規模は約 {size:,} Rust行 (全辺の追加行 + 削除行の1/4。複数の辺で触れる箇所は重複計上されるので上限の目安)。"))
    w(t(f"- Eras by class: seedrandom `S` {era_classes['S']} classes ({era_builds['S']} builds, {100 * era_weight['S'] / tot:.0f}% of weight), sfc32 without DetMath `F` {era_classes['F']} ({era_builds['F']}, {100 * era_weight['F'] / tot:.0f}%), sfc32 with DetMath `D` {era_classes['D']} ({era_builds['D']}, {100 * era_weight['D'] / tot:.0f}%). The baseline is era D, the only era that does not depend on the host `Math.*` (see Node/V8). Era-changing edges: {', '.join(boundaries)}.",
        f"- 世代別クラス数: seedrandom `S` {era_classes['S']} ({era_builds['S']}ビルド、重み{100 * era_weight['S'] / tot:.0f}%)、DetMathなしsfc32 `F` {era_classes['F']} ({era_builds['F']}、{100 * era_weight['F'] / tot:.0f}%)、DetMathありsfc32 `D` {era_classes['D']} ({era_builds['D']}、{100 * era_weight['D'] / tot:.0f}%)。基準は世代Dで、実行環境の `Math.*` に依存しない唯一の世代 (Node/V8の節を参照)。世代が変わる辺: {', '.join(boundaries)}。"))
    w(t("- Weights are estimated from `Index.csv` (2,088 sampled records, not the full public volume): each UTC day counts as 1, split among the builds in proportion to their Public samples of that day (all samples if none is Public). The sum over all classes is %.0f day-equivalents. Real game counts per day are not in the index; multiply by the daily public game count for a game estimate." % tot,
        "- 重みは `Index.csv` (サンプリングした2,088件であり、Publicの全件ではない) から推定: UTCの各日を1とし、その日のPublicサンプル数の比で各ビルドに按分する (Publicが無ければ全サンプル)。全クラス合計は %.0f 日換算。1日あたりの実ゲーム数はインデックスに無いので、ゲーム数が要るなら日次Public件数を掛ける。" % tot))
    w(t("- Hash origin: `turn.hash` in records is produced by the clients (browser workers), not by the server; the server stores the majority value. Until v0.34.25 the server never runs the engine; from v0.35.0 (PR #5807) it replays games in a subprocess only to settle disputed winner votes (no hashes are written there).",
        "- ハッシュの生成元: レコードの `turn.hash` はクライアント (ブラウザのワーカー) が計算した値で、サーバーは多数決の値を保存するだけ。v0.34.25まではサーバーはエンジンを実行せず、v0.35.0以降 (PR #5807) は勝者投票が割れたときの判定のためだけにサブプロセスでゲームを再生する (そこでハッシュは書かれない)。"))
    w(t("- V8 finding: every build's Dockerfile uses `node:24-slim` (V8 13.6), where `Math.pow` is already `std::pow` (platform libm) while exp/log/atan2/sin/cos are still fdlibm ports; V8 15.0 switches them all to LLVM libc. Because clients, not the server, produce the hashes, what matters is the players' browsers; see the Node/V8 section.",
        "- V8の調査結果: 全ビルドのDockerfileは `node:24-slim` (V8 13.6) で、そこでは `Math.pow` が既に `std::pow` (プラットフォームのlibm)、exp/log/atan2/sin/cosはfdlibm移植のまま。V8 15.0で全てLLVM libcに切り替わる。ハッシュを作るのはサーバーでなくクライアントなので、重要なのはプレイヤーのブラウザ。Node/V8の節を参照。"))
    w("")
    w("## " + t("Method", "方法"))
    w("")
    w(t("1. Core roots per commit: `src/core/` before the monorepo split (upstream e82286146, first seen in v0.35.0), and `packages/{engine,engine-lib,engine-api,shared}/src/` after it. The root prefix is stripped, so moved files keep their key (no key collisions in any build; asserted).",
        "1. コミットごとのcore: モノレポ分割 (upstream e82286146、v0.35.0で初出) 前は `src/core/`、後は `packages/{engine,engine-lib,engine-api,shared}/src/`。ルートの接頭辞を除いたパスをキーにするので、移動したファイルは同じキーになる (全ビルドでキー衝突なし、assert済み)。"))
    w(t("2. Excluded from the hash (wire/API schemas, client views, themes, asset or transport helpers, serialisation): `ApiSchemas`, `ClanApiSchemas`, `CosmeticSchemas`, `StatsSchemas`, `WireSchemas`, `WorkerSchemas`, `ZbinWire`, `LevelBadgeWire`, `AssetPaths`, `AssetUrls`, `Base64`, `CloseCodes`, `ClusterConfig`, `CustomFlag`, `EventBus`, `Format`, `PatternDecoder`, `ServerList`, `MapPlayerCounts`, `FetchGameMapLoader`, `GameMapLoader`, `game/GameView`, `game/ReadViews`, `game/NameBoxCalculator`, `game/UserSettings`, `game/SnapshotHeader`, `configuration/PastelTheme*`, `configuration/Colors`, `configuration/ColorAllocator`, `utilities/DebugSpan`, `worker/*`, `snapshot/*` (save/restore codec, reads state only). Everything else stays in, including `Schemas.ts` (zod defaults of the game config), `configuration/*Config.ts` (Dev/Prod overrides), `Maps.gen.ts`, name tables, `GameUpdates` and `validations/*`. When unsure a file is included, so classes may be split more finely than strictly necessary but are never merged wrongly.",
        "2. ハッシュから除外 (ワイヤ/APIスキーマ、クライアント表示、テーマ、アセット/通信補助、シリアライズ): `ApiSchemas`、`ClanApiSchemas`、`CosmeticSchemas`、`StatsSchemas`、`WireSchemas`、`WorkerSchemas`、`ZbinWire`、`LevelBadgeWire`、`AssetPaths`、`AssetUrls`、`Base64`、`CloseCodes`、`ClusterConfig`、`CustomFlag`、`EventBus`、`Format`、`PatternDecoder`、`ServerList`、`MapPlayerCounts`、`FetchGameMapLoader`、`GameMapLoader`、`game/GameView`、`game/ReadViews`、`game/NameBoxCalculator`、`game/UserSettings`、`game/SnapshotHeader`、`configuration/PastelTheme*`、`configuration/Colors`、`configuration/ColorAllocator`、`utilities/DebugSpan`、`worker/*`、`snapshot/*` (保存/復元コーデックで状態を読むだけ)。それ以外は含める。`Schemas.ts` (ゲーム設定のzodデフォルト)、`configuration/*Config.ts` (Dev/Prodの上書き)、`Maps.gen.ts`、名前表、`GameUpdates`、`validations/*` も含む。迷ったら含めるので、クラスが必要以上に細かく分かれることはあっても誤って併合されることはない。"))
    w(t("3. Maps: `resources/maps/<map>/{map.bin,map4x.bin,map16x.bin,mini_map.bin,manifest.json}` are hashed per build for every map directory; map changes are reported per edge and do not split classes. Tournament maps (`tourney2teams`, `tourney3teams`, `tourney4teams`, `tourney8teams`) have no directory of their own (7 build/map pairs); they are resolved through the build's map loader and are not covered by the per-map hashes.",
        "3. マップ: `resources/maps/<map>/{map.bin,map4x.bin,map16x.bin,mini_map.bin,manifest.json}` を全マップディレクトリについてビルドごとにハッシュ化する。マップの変更は辺ごとに報告し、クラスは分割しない。トーナメント系マップ (`tourney2teams`、`tourney3teams`、`tourney4teams`、`tourney8teams`) は専用ディレクトリが無く (7組)、そのビルドのマップローダー経由で解決されるため、マップごとのハッシュには含まれない。"))
    w(t("4. Tree: candidate edges are the 7 classes with the fewest differing files plus the temporal neighbours; the edge cost is the number of changed lines (`git diff --numstat` per differing blob, whole file counted for added/removed files). A minimum spanning tree rooted at the baseline (Prim) gives the parents. The ids C000.. are the porting order: from the root, the frontier node with the highest weight per changed line is taken next, so heavy classes near the baseline come first, then the tree is walked towards older and newer builds.",
        "4. 木: 候補辺は差分ファイル数が最小の7クラスと時間的に隣接するクラス。辺のコストは変更行数 (差分のあるblobごとの `git diff --numstat`、追加/削除ファイルは全行)。基準を根とする最小全域木 (Prim) で親を決める。ID C000.. は移植順で、根から、変更行あたりの重みが最大のフロンティアのノードを次に取る。つまり基準近傍の重いクラスが先で、その後に新旧両方向へ木をたどる。"))
    w(t("5. Edge summary: functions are the enclosing class/method of each hunk in the child file (heuristic); PRs are the `(#NNNN)` numbers in the subjects of the commits touching the core between the two representative commits (symmetric difference, so release-branch cherry-picks appear on both sides); the classification comes from the file path and a numeric-literal-only test. Size estimate = added lines + deleted lines / 4. Risk: high if a PRNG/DetMath file changed, a file is a rewrite, or more than 2,000 lines differ; medium for AI or pathfinding changes or more than 300 lines; low otherwise.",
        "5. 辺の要約: 関数は子側ファイルの各ハンクを含むクラス/メソッド (ヒューリスティック)、PRは2つの代表コミット間でcoreに触れたコミットの件名の `(#NNNN)` (対称差なのでリリースブランチへのチェリーピックは両側に出る)、分類はファイルパスと数値リテラルのみの差分判定。規模の推定 = 追加行 + 削除行/4。リスク: PRNG/DetMathのファイル変更、書き直し、2,000行超の差分のいずれかで高、AI/経路探索の変更または300行超で中、それ以外は低。"))
    w("")
    w("## " + t("Top-weighted classes", "重みの大きいクラス"))
    w("")
    w("| " + t("Class | Builds | Weight (day-eq, share) | Era | Risk | Diff lines", "クラス | ビルド | 重み (日換算, 割合) | 世代 | リスク | 差分行") + " |")
    w("|---|---|---|---|---|---|")
    w(top_rows)
    w("")
    w(t(f"The two heaviest classes ({top[0]['id']} and {top[1]['id']}) carry {top2:.0f}% of the sampled days; each differs from its parent by thousands of lines, so port such classes as side-by-side variants rather than patches. The baseline class C000 itself is only {100 * byid['C000']['weight'] / tot:.1f}% of the weight, so replaying the full history needs the whole tree.",
        f"重い上位2クラス ({top[0]['id']}、{top[1]['id']}) でサンプル日の{top2:.0f}%を占める。どちらも親との差分が数千行なので、パッチではなく並置したバリアントとして移植する。基準クラスC000自体の重みは{100 * byid['C000']['weight'] / tot:.1f}%に過ぎず、全履歴の再生には木全体が必要になる。"))
    w("")
    w("## " + t("Classes", "クラス表"))
    w("")
    w(t("Columns: Weight = day-equivalents / Public records in the index; Cum.% = cumulative weight in porting order; Era: S seedrandom, F sfc32 without DetMath, D sfc32 with DetMath; Diff lines = added + deleted in the simulation files against the parent class representative; est. Rust lines = added + deleted/4; Map files changed = maps whose files differ from the parent representative (all maps in the tree, not only those played). The builds of a class are listed oldest first; the first listed build of C000 is the oldest build with the baseline code. Status = porting state in `Rules.rs` and how many sampled records of the class replay with every hash matching (hand-maintained in `Docs/BuildStatus.json`).",
        "列: 重み = 日換算 / インデックス内のPublicレコード数、累積% = 移植順の累積重み、世代: S seedrandom、F DetMathなしsfc32、D DetMathありsfc32、差分行 = 親クラスの代表との間のシミュレーションファイルの追加+削除行、推定Rust行 = 追加 + 削除/4、変更マップファイル = 親の代表と異なるマップ (木に含まれる全マップ、プレイされたものに限らない)。クラス内のビルドは古い順。状態 = `Rules.rs` での移植状況と、そのクラスのサンプルレコードのうち全ハッシュが一致した件数 (`Docs/BuildStatus.json` に手書きで保持)。"))
    w("")
    w(table)
    w("")
    w("## " + t("Large rewrites (prefer side-by-side implementations)", "大規模な書き直し (並置実装を推奨)"))
    w("")
    w(t("Flagged when a file is added or removed with more than 300 lines, or an edit changes more than 400 lines and more than half of the file.",
        "300行超のファイルの追加/削除、または400行超かつファイルの半分超を変更する編集を対象とする。"))
    w("")
    w("| " + t("Class | Parent | File | Kind | Lines", "クラス | 親 | ファイル | 種別 | 行数") + " |")
    w("|---|---|---|---|---|")
    w(rew_rows)
    w("")
    w(t(f"Notes: `game/Maps.gen.ts` rewrites are generated data tables (map list), cheap to port by regenerating. The largest edges by lines are listed in the class table; C059 (first seen {gen('C059')}) is the biggest nation/FakeHuman restructuring (many 300 to 1,000 line files added and removed), C081 (first seen {gen('C081')}) is the monorepo split with `EngineConfig`, `GameMapImpl` and `GameTypes` extracted (mostly moved code), C083 (first seen {gen('C083')}) splits the nation attack, nuke and structure behaviour into new files, and C088 (first seen {gen('C088')}) rewrites `PlayerExecution`, `RailNetworkImpl` and adds `PackedUnitUpdates`.",
        f"補足: `game/Maps.gen.ts` の書き直しは生成されるデータ表 (マップ一覧) で、再生成で済むため安い。行数の大きい辺はクラス表に載っている。C059 (初出 {gen('C059')}) はnation/FakeHumanの最大の再構成 (300〜1,000行のファイルが多数追加/削除)、C081 (初出 {gen('C081')}) は `EngineConfig`、`GameMapImpl`、`GameTypes` を抽出したモノレポ分割 (大半は移動コード)、C083 (初出 {gen('C083')}) はnationの攻撃/核/建設の挙動を新ファイルに分割、C088 (初出 {gen('C088')}) は `PlayerExecution`、`RailNetworkImpl` の書き直しと `PackedUnitUpdates` の追加。"))
    w("")
    w("## " + t("Main risks", "主なリスク"))
    w("")
    w(t(f"1. Pre-DetMath eras (S and F, {era_builds['S'] + era_builds['F']} of 159 builds, {100 * (era_weight['S'] + era_weight['F']) / tot:.0f}% of the weight) call `Math.pow`/`Math.exp` in core formulas (for example troop growth `Math.pow(troops, 0.73)`, structure cost `Math.pow(2, n)` and the logistic curve in `Util.ts`). Their recorded hashes are the clients' majority and depend on the clients' JS engines (below); a bit-exact Rust port needs the right libm per build, which cannot be known without trial against the records. Mitigation: replay the sampled records of those builds with Node 22 (legacy fdlibm pow) and Node 24 (`std::pow`) in the reference tracer and keep the variant that matches; expect some builds to be unreproducible if the majority engines disagreed.",
        f"1. DetMath以前の世代 (SとF、159ビルド中{era_builds['S'] + era_builds['F']}、重みの{100 * (era_weight['S'] + era_weight['F']) / tot:.0f}%) はcoreの式で `Math.pow`/`Math.exp` を呼ぶ (例: 兵力成長 `Math.pow(troops, 0.73)`、施設コスト `Math.pow(2, n)`、`Util.ts` のロジスティック曲線)。記録されたハッシュはクライアントの多数決で、クライアントのJSエンジンに依存する (後述)。ビット一致のRust移植にはビルドごとに正しいlibmが必要で、レコードとの照合なしには決められない。対策: それらのビルドのサンプルレコードをリファレンストレーサーでNode 22 (旧fdlibmのpow) とNode 24 (`std::pow`) の両方で再生し、一致する方を採用する。多数派のエンジンが割れていたビルドは再現できない可能性がある。"))
    w(t(f"2. The seedrandom-to-sfc32 switch needs an ARC4/seedrandom port for era S ({era_builds['S']} builds, {100 * era_weight['S'] / tot:.0f}% of the weight). Edges touching `PseudoRandom.ts` or `DetMath.ts`: {', '.join(prng_edges)}; `PseudoRandom.ts` also changes on release-branch edges without changing era, so diff those edges for seed or warm-up changes before assuming the algorithm is the same.",
        f"2. seedrandomからsfc32への切替では、世代S ({era_builds['S']}ビルド、重みの{100 * era_weight['S'] / tot:.0f}%) のためにARC4/seedrandomの移植が必要。`PseudoRandom.ts` または `DetMath.ts` に触れる辺: {', '.join(prng_edges)}。リリースブランチ上の辺では世代が変わらなくても `PseudoRandom.ts` が変わることがあるので、アルゴリズムが同じと決める前にシード/ウォームアップの変更を差分で確認する。"))
    w(t("3. Sampled records are few for most classes (many builds have 3 records), so a class can be ported and still fail on a rarely exercised branch. Port in the order given and keep every class's records as regression tests.",
        "3. ほとんどのクラスはサンプルレコードが少なく (多くのビルドが3件)、移植しても稀にしか通らない分岐で失敗しうる。示した順に移植し、各クラスのレコードを回帰テストとして残す。"))
    w(t("4. Maps change often: %d `map.bin` variants over %d maps (%d maps changed at least once), %d `manifest.json` variants. Replays must load the map files of the record's own build (stored by content hash), not the baseline's. The oldest builds also ship `mini_map.bin` instead of `map4x.bin`/`map16x.bin` (%d maps). Not every map variant is used by a record."
        % (ms["map.bin"]["variants"], ms["map.bin"]["maps"], ms["map.bin"]["changed_maps"], ms["manifest.json"]["variants"], ms["mini_map.bin"]["maps"]),
        "4. マップは頻繁に変わる: %d マップで `map.bin` が %d 種類 (少なくとも1回変わったマップは %d)、`manifest.json` は %d 種類。再生にはレコード自身のビルドのマップファイル (内容ハッシュで保存) を読み込む必要があり、基準のものは使えない。最古のビルドは `map4x.bin`/`map16x.bin` の代わりに `mini_map.bin` を持つ (%d マップ)。全てのマップ種類がレコードで使われるわけではない。"
        % (ms["map.bin"]["maps"], ms["map.bin"]["variants"], ms["map.bin"]["changed_maps"], ms["manifest.json"]["variants"], ms["mini_map.bin"]["maps"])))
    w(t("5. Release branches: many builds are not on main (Builds.md), so the tree mixes main-line and branch commits and the PR list of an edge can include PRs that belong to the other side of the edge. Selecting the engine by `gitCommit` (class table in `BuildClasses.toml`) is exact; any date-based selection is not.",
        "5. リリースブランチ: mainに無いビルドが多い (Builds.md) ため、木にはmain系列とブランチのコミットが混在し、辺のPR一覧には実際には辺の反対側にあるPRが含まれうる。`gitCommit` によるエンジン選択 (`BuildClasses.toml` のクラス表) は正確だが、日付による選択は不正確。"))
    w(t("6. Schema-only edits are included in the hash by design (see Method 2), so some small classes differ from their parent only by config-schema or name-table changes; these are cheap but cannot be merged without checking.",
        "6. スキーマのみの編集も設計上ハッシュに含まれる (方法2を参照)。そのため一部の小さなクラスは設定スキーマや名前表の変更だけで親と異なる。安価だが、確認なしには併合できない。"))
    w("")
    w("## " + t("Node / V8 and the origin of the hashes", "Node / V8 とハッシュの生成元"))
    w("")
    w("### " + t("Who computes `turn.hash`", "`turn.hash` を計算するのは誰か"))
    w("")
    w(t("- `GameImpl.hash()` runs inside the engine in every client's worker. Clients send `{type:\"hash\", turnNumber, hash}` messages; `GameServer` keeps them per client (`client.hashes`), `DesyncDetector.findOutOfSyncClients` takes the most common value, and `GameServer` writes `this.turns[turn].hash = mostCommonHash` (baseline `src/server/GameServer.ts:1999`; v0.25.20 `GameServer.ts:712`). Clients that disagree are flagged as desynced.",
        "- `GameImpl.hash()` は各クライアントのワーカー内のエンジンで実行される。クライアントが `{type:\"hash\", turnNumber, hash}` を送り、`GameServer` が `client.hashes` に保持し、`DesyncDetector.findOutOfSyncClients` が最頻値を取り、`GameServer` が `this.turns[turn].hash = mostCommonHash` に書き込む (基準の `src/server/GameServer.ts:1999`、v0.25.20の `GameServer.ts:712`)。一致しないクライアントはdesyncとして扱われる。"))
    w(t("- Checked builds (v0.25.20, v0.34.22): the server code never imports `GameRunner` or calls `executeNextTick`. From v0.35.0 (PR #5807, 2026-10-06; ancestor of every build from `dfd420fd7836` on, not of v0.34.x) `GameServer` runs `replayWinnerInChild` (`packages/engine/src/WinnerReplay.ts`, one subprocess) to settle disputed winner votes; it reports winner and stats only. Singleplayer records carry the hash of the single local client.",
        "- 確認したビルド (v0.25.20、v0.34.22) ではサーバーコードは `GameRunner` をimportせず `executeNextTick` も呼ばない。v0.35.0 (PR #5807、2026-10-06。`dfd420fd7836` 以降の全ビルドの祖先で、v0.34.xには含まれない) から `GameServer` は勝者投票が割れたとき `replayWinnerInChild` (`packages/engine/src/WinnerReplay.ts`、サブプロセス1つ) を実行するが、報告するのは勝者と統計のみ。Singleplayerのレコードは唯一のローカルクライアントのハッシュを持つ。"))
    w(t("- Consequence: the Node version in the server Dockerfile does not influence the recorded hashes. What matters is the JS engine of the majority of the players: mostly Chrome-family V8 (Chrome milestone N ships V8 N/10, for example Chrome 141 = V8 14.1), plus Firefox (SpiderMonkey) and Safari (JavaScriptCore), whose libm differs again. A recorded majority on pre-DetMath builds means most clients agreed bit for bit on every sampled turn; upstream still added DetMath (PR #5191, first build v0.34.0) because last-bit differences between engines are real desyncs (the DetMath header comment says so).",
        "- 結論: サーバーのDockerfileのNodeバージョンは記録されたハッシュに影響しない。重要なのは多数派プレイヤーのJSエンジンで、主にChrome系のV8 (ChromeのマイルストーンNはV8 N/10、例: Chrome 141 = V8 14.1)、それにFirefox (SpiderMonkey) とSafari (JavaScriptCore) で、libmはそれぞれ異なる。DetMath以前のビルドで多数派が記録されているのは、大半のクライアントが各ターンでビット単位で一致していたことを意味する。それでもupstreamがDetMath (PR #5191、初出ビルドv0.34.0) を入れたのは、エンジン間の最下位ビットの差が実際のdesyncになるため (DetMathのヘッダコメントにある)。"))
    w("")
    w("### " + t("Node per build", "ビルドごとのNode"))
    w("")
    w(t("All 159 builds have `FROM node:24-slim` in the Dockerfile (a floating tag, so the minor version is whatever Docker Hub served at image build time; no `.nvmrc` exists in any build). `package.json` `engines` (`node >=24.15.0 <25`, `npm >=12.1.0 <13`) appears only in the 20 newest builds. The Node major is therefore 24 for the whole period.",
        "159ビルド全てのDockerfileが `FROM node:24-slim` (浮動タグなので、マイナーバージョンはイメージビルド時にDocker Hubが返したもの。どのビルドにも `.nvmrc` は無い)。`package.json` の `engines` (`node >=24.15.0 <25`、`npm >=12.1.0 <13`) は最新の20ビルドにのみある。つまりNodeのメジャーは全期間24。"))
    w("")
    w("| Node / V8 | `Math.pow` | `exp` `log` `atan2` | `sin` `cos` |")
    w("|---|---|---|---|")
    w("| Node 20.x = V8 11.3 | fdlibm port (`ieee754::legacy::pow`) | fdlibm port | fdlibm port |")
    w("| Node 22.x = V8 12.4 (" + t("local reference", "ローカルのリファレンス") + ") | fdlibm port | fdlibm port | fdlibm port |")
    w("| Node 24.x = V8 13.6 (" + t("all builds' Dockerfile", "全ビルドのDockerfile") + ") | " + t("**`std::pow` (platform libm)**; flag `use_std_math_pow` defaults to true since V8 13.3 (not AIX); `x**2` and `x**0.5` special-cased", "**`std::pow` (プラットフォームのlibm)**。フラグ `use_std_math_pow` はV8 13.3以降既定true (AIXを除く)。`x**2` と `x**0.5` は特別扱い") + " | fdlibm port | " + t("fdlibm port (`V8_USE_LIBM_TRIG_FUNCTIONS` not found in Node's v8 gyp files; V8's GN build has `v8_use_libm_trig_functions` for glibc-derived sin/cos in Chrome)", "fdlibm移植 (NodeのV8 gypファイルに `V8_USE_LIBM_TRIG_FUNCTIONS` は無い。V8のGNビルドには `v8_use_libm_trig_functions` があり、Chromeではglibc由来のsin/cos)") + " |")
    w("| Node 25.x = V8 14.1 | `std::pow` | fdlibm port | fdlibm port |")
    w("| V8 14.6 to 14.9 | `std::pow` | fdlibm port (six functions unchanged; other functions trimmed) | fdlibm port |")
    w("| V8 15.0 and later | " + t("LLVM libc `pow`", "LLVM libc `pow`") + " | LLVM libc | LLVM libc |")
    w("")
    w(t("Sources: `src/base/ieee754.cc`, `src/numbers/ieee754.cc` and `src/flags/flag-definitions.h` at V8 11.3.244.8, 12.4.254.21, 13.0 to 13.6 (lkgr branches and 13.6.233.10), 14.1 to 14.9 and 15.0 lkgr, 15.2, 15.5, 15.9 and `main` (V8 16.0), plus Node `deps/v8/include/v8-version.h` on v20.x to v25.x. The V8 `ieee754.cc` is 3,036 lines up to 14.5, 2,969 lines at 14.8 and 14.9, 382 lines at 15.0 (wrapping `LIBC_NAMESPACE::shared::{acos,asin,atan,atan2,cbrt,cos,exp,expm1,log,log10,log1p,log2,pow,sin,tan}` from `third_party/llvm-libc`) and 245 lines on main. The exact commit of the switch was not bisected (it lies between the 14.9 and 15.0 branches); the Chrome stable date for 15.0 (Chrome 150) was not verified here; with a 4-week cadence from Chrome 144 (January 2026) it would be about mid 2026, which fits DetMath arriving on 2026-08-30.",
        "出典: V8 11.3.244.8、12.4.254.21、13.0〜13.6 (lkgrブランチと13.6.233.10)、14.1〜14.9、15.0 lkgr、15.2、15.5、15.9、`main` (V8 16.0) の `src/base/ieee754.cc`、`src/numbers/ieee754.cc`、`src/flags/flag-definitions.h`、およびNode v20.x〜v25.x の `deps/v8/include/v8-version.h`。V8の `ieee754.cc` は14.5まで3,036行、14.8と14.9で2,969行、15.0で382行 (`third_party/llvm-libc` の `LIBC_NAMESPACE::shared::{acos,asin,atan,atan2,cbrt,cos,exp,expm1,log,log10,log1p,log2,pow,sin,tan}` を包む)、mainで245行。切替の正確なコミットは二分探索していない (14.9と15.0のブランチの間)。15.0 (Chrome 150) のChrome安定版の日付はここでは未検証で、Chrome 144 (2026年1月) からの4週間周期なら2026年半ば頃となり、DetMathが2026-08-30に入ったことと整合する。"))
    w("")
    w(t("Implications for the V8 fdlibm port:", "V8 fdlibm移植への含意:"))
    w("")
    w(t("- The six fdlibm routines (`exp`, `log`, `atan2`, `sin`, `cos`, `pow`) exist unchanged in V8 up to 14.9, so one port of `ieee754.cc` (V8 12.4 or 13.6) covers all fdlibm behaviour; `pow` is `legacy::pow` there.",
        "- 6つのfdlibmルーチン (`exp`、`log`、`atan2`、`sin`、`cos`、`pow`) はV8 14.9まで変更なしで存在するので、`ieee754.cc` (V8 12.4または13.6) を1つ移植すればfdlibmの挙動は全て網羅できる。`pow` はそこでは `legacy::pow`。"))
    w(t("- `Math.pow` is the outlier: from V8 13.3 it is the platform `std::pow` (glibc on Linux, UCRT on Windows, Apple libm on macOS; none is fdlibm). Clients on Chrome 133 or later (February 2025, i.e. the whole period) therefore produced pow results from whatever libm they ran on; clients on older V8 used legacy fdlibm pow. The port should make the pow implementation a per-build choice (`fdlibm`, a correctly rounded glibc-style `pow`, or LLVM libc) selected by the trial against records described above.",
        "- `Math.pow` は例外: V8 13.3以降はプラットフォームの `std::pow` (Linuxではglibc、WindowsではUCRT、macOSではApple libm。どれもfdlibmではない)。したがってChrome 133以降 (2025年2月、つまり全期間) のクライアントのpowの結果は、実行環境のlibm次第だった。古いV8のクライアントは旧fdlibmのpowを使った。移植ではpowの実装をビルドごとの選択肢 (`fdlibm`、glibc型の正しく丸められる `pow`、LLVM libc) とし、上記のレコードとの照合で選ぶ。"))
    w(t(f"- From V8 15.0 (Chrome 150, likely mid 2026 onward) clients use LLVM libc for all six functions (correctly rounded in almost all cases, differing from fdlibm in the last bit now and then). Era F builds seen on or after 2026-07-01 are the most exposed: classes {', '.join(late_f) if late_f else 'none'}.",
        f"- V8 15.0 (Chrome 150、おそらく2026年半ば以降) からクライアントは6関数ともLLVM libcを使う (ほぼ常に正しく丸められ、fdlibmとは最下位ビットでときどき異なる)。2026-07-01以降に見られる世代Fのビルドが最も影響を受ける: クラス {', '.join(late_f) if late_f else 'なし'}。"))
    w(t("- Local reference note: the baseline worktree runs Node 22 (V8 12.4, legacy fdlibm pow) although production used Node 24 and, more to the point, players' browsers. This does not matter for era D (no host transcendental calls); for eras S and F run the reference under Node 22 and 24 and compare with the record hashes.",
        "- ローカルのリファレンスについて: 基準ワークツリーはNode 22 (V8 12.4、旧fdlibmのpow) で動くが、本番はNode 24で、さらに言えばプレイヤーのブラウザだった。世代D (ホストの超越関数を呼ばない) では問題にならない。世代SとFではNode 22と24でリファレンスを実行し、レコードのハッシュと比較する。"))
    w("")
    w("## " + t("Map-file changes", "マップファイルの変更"))
    w("")
    w(t("Per-class map changes are in the last column of the class table and in `maps_changed` of `BuildClasses.toml` (`<map>:<file>`). Totals over the 159 builds (distinct content hashes per map and file): `map.bin` %d variants / %d maps (%d maps changed), `map4x.bin` %d / %d (%d), `map16x.bin` %d / %d (%d), `manifest.json` %d / %d (%d). A changed `map.bin` changes terrain and must be loaded for replays of that build; many `manifest.json`-only changes are metadata. Plan: store map files by content hash and let `BuildClasses.toml` plus a per-build map index select them."
        % (ms["map.bin"]["variants"], ms["map.bin"]["maps"], ms["map.bin"]["changed_maps"], ms["map4x.bin"]["variants"], ms["map4x.bin"]["maps"], ms["map4x.bin"]["changed_maps"], ms["map16x.bin"]["variants"], ms["map16x.bin"]["maps"], ms["map16x.bin"]["changed_maps"], ms["manifest.json"]["variants"], ms["manifest.json"]["maps"], ms["manifest.json"]["changed_maps"]),
        "クラスごとのマップ変更は表の最後の列と `BuildClasses.toml` の `maps_changed` (`<map>:<file>`) にある。159ビルドの合計 (マップとファイルごとの異なる内容ハッシュ数): `map.bin` %d種類 / %dマップ (変更ありは%dマップ)、`map4x.bin` %d / %d (%d)、`map16x.bin` %d / %d (%d)、`manifest.json` %d / %d (%d)。`map.bin` の変更は地形を変えるため、そのビルドの再生ではそれを読み込む必要がある。`manifest.json` だけの変更の多くはメタデータ。方針: マップファイルは内容ハッシュで保存し、`BuildClasses.toml` とビルドごとのマップ索引で選ぶ。"
        % (ms["map.bin"]["variants"], ms["map.bin"]["maps"], ms["map.bin"]["changed_maps"], ms["map4x.bin"]["variants"], ms["map4x.bin"]["maps"], ms["map4x.bin"]["changed_maps"], ms["map16x.bin"]["variants"], ms["map16x.bin"]["maps"], ms["map16x.bin"]["changed_maps"], ms["manifest.json"]["variants"], ms["manifest.json"]["maps"], ms["manifest.json"]["changed_maps"])))
    w("")
    w("## " + t("Regenerating", "再生成"))
    w("")
    w("```sh\npython3 Tools/BuildClassify.py --upstream Cache/OpenFrontIO --records Cache/Records --out builds.json\npython3 Tools/BuildPlanGen.py --upstream Cache/OpenFrontIO --in builds.json --toml Crates/Engine/src/Builds/BuildClasses.toml --md-data md.json\npython3 Tools/BuildPlanDoc.py --upstream Cache/OpenFrontIO --builds builds.json --data md.json --out-dir Docs\n```")
    w("")
    return "\n".join(out)
