# 検証実行系再設計

## Status

Accepted（Phase 2 一部採用）

phase 1 として `Native Seed Regression Runner` は採用済み。
実装判断は `docs/decisions/260421-native-seed-regression-runner.md` を参照する。
phase 2 として `VerificationMode` 導入、`HeadlessMetrics` での post-step 停止、
`pending_post_step` の実行系整理（Finalize完了時の即時 post-step）を採用した。
`ScientificBenchmark` は初版として専用サンプル蓄積フックを実装し、
詳細artifact収集は後続段階とする。

## 背景

- 現行の検証は大きく `Rust unit test`、`WASM API contract test`、`seed regression`、`perf gate`、実データ比較ベンチに分かれている。
- ただし実行経路としては、`seed regression` と `perf gate` がともに `WASM build -> Node/TS -> WASM controller` に強く寄っており、シミュレーション本体の回帰確認と transport/UI 都合が十分に分離されていない。
- 直近の perf baseline では `tick_total.mean` は約 `558ms`、内訳は `exec_world.mean` 約 `539ms`、`delta_sync.mean` 約 `19ms` である。
- `exec_world` 内では特に `step_geology_terrain`、`step_geology_river`、`step_observe_world_change` の比率が高く、モデル計算と観測同期がどちらも主要コストになっている。
- 現在のレイヤ構造 (`core -> application -> transport -> presentation`) 自体は整理されているが、検証実行面では `simulation verification`、`API verification`、`UI/perf verification` の責務境界がまだ薄い。

## 目的

- 日常開発で回す検証の待ち時間を大きく下げる。
- 「モデルが壊れた」のか「transport が壊れた」のか「UI/perf が壊れた」のかを分離して判定できるようにする。
- Docs-first 方針と現在のレイヤ構造を維持したまま、検証専用の実行経路を明示する。
- 科学シミュレーションとして必要な seed 回帰、性能監視、実データ比較をそれぞれ適切なコストで維持する。

## 提案概要

### 1. 検証を3系統に分離する

検証対象を次の3系統に分離する。

- `simulation verification`
    - 世界状態と指標の回帰確認
    - 正本の実行経路は Rust native とする
- `interface verification`
    - WASM API 契約、Worker protocol、CLI 引数などの境界確認
    - 正本の実行経路は transport adapter ごとに持つ
- `presentation/perf verification`
    - delta 同期、描画向け更新、UI loop を含む perf と統合確認
    - 正本の実行経路は worker/UI 寄りの path とする

`seed regression` は原則として `simulation verification` に属し、WASM build を常用経路にしない。

### 2. 検証専用 runtime を導入する

`core` と `application` の上に、検証専用の実行面を置く。

```text
core-sim
  World / module DAG / deterministic tick / snapshot

application
  world init / slice exec / metrics query / replay

verification runtime
  scenario runner / baseline compare / tolerance policy / perf probe

adapters
  wasm / worker / cli / node
```

ここでいう `verification runtime` は新しい UI 層ではなく、headless 実行・比較・集計を担う運用面の実体である。

### 3. 日常回す回帰は Rust native を正本にする

次を採用する。

- `seed regression quick/heavy` の主経路は Rust native CLI に移す
- WASM build を伴う回帰は `API contract` と `transport integration` に限定する
- Node/TS は結果整形や可視化に使ってよいが、シミュレーション回帰の必須経路にはしない

理由:

- seed 回帰の本質は transport 契約ではなく、世界更新の決定性と許容差分監視である
- 現状の `WASM build -> Node -> WASM` は待ち時間と故障点を増やす
- Rust native に寄せると、ビルド済み差分の利用と headless 実行がしやすい

### 4. 検証モードを明示化する

単一の world 実行経路にすべての観測を載せず、次の実行モードを導入する。

- `Interactive`
    - delta 同期、history、UI 向け観測を有効
- `HeadlessMetrics`
    - 回帰判定に必要な metrics のみ収集
- `ScientificBenchmark`
    - 実データ比較用の詳細 field / artifact を収集

これにより、日常の回帰確認で `observe world change` や UI 向け delta 生成を常に走らせる構造を避ける。

### 5. 観測対象を tier 化する

観測を常に「全 field」前提にせず、目的ごとに観測セットを固定する。

- `smoke`
    - `tick`、`era`、主要 invariant のみ
- `regression`
    - `land_cells`、`height_mean`、`height_std`、`max_river_flux` などの少数指標
- `module diagnostics`
    - module 別の内部メトリクス、rebuild/fallback count
- `ui delta`
    - presentation に必要な dirty field のみ
- `science benchmark`
    - 実データ比較に必要な field 一式

### 6. metrics-first な回帰判定へ寄せる

回帰判定のために毎回広い観測・同期を行うのではなく、module 実行時に軽量 metrics reducer を更新する。

例:

- river 上位流量和
- 大陸数
- 最大連結成分サイズ
- 再構築回数
- changed ratio
- fallback 回数

これにより、`verification` では「world 全体を観測してから指標化する」のではなく「指標を保ちながら world を進める」形に寄せる。

### 7. post-step を phase model に寄せる

現行の `pending_post_step` による別処理は、slice 実行と profiling の理解を難しくする。

次を検討対象とする。

- `observe`
- `history snapshot`
- 軽量 sync

を `Finalize` 配下または検証モード別の宣言済み phase として扱う。

これにより、実行 budget・profiling・検証モード切替を同じ execution model で扱えるようにする。

## スコープ

この proposal で決めること:

- 検証を `simulation / interface / presentation-perf` に分離する方針
- `seed regression` の正本実行経路を Rust native に寄せる方針
- `VerificationMode` と観測 tier の導入方針
- metrics-first な回帰判定へ寄せる方針
- 検証ジョブの運用層を再編する方針

この proposal でまだ決めないこと:

- Rust native CLI の最終コマンド体系
- crate 分割をどこまで行うか
- `pending_post_step` を完全吸収する最終 API 形状
- 各 module reducer の詳細実装
- CI workflow の最終トリガとジョブ分割

## 成功条件

- 日常回す `quick regression` が現行より有意に短くなる。
- シミュレーション回帰で WASM build を必須としない常用経路を持つ。
- `simulation regression`、`API contract`、`UI/perf regression` の失敗原因を別々に報告できる。
- `seed regression` の許容差分運用は維持される。
- `perf gate` で少なくとも `exec_world` と `observation/sync` の寄与を分離して追跡できる。
- Docs-first の流れを崩さず、`reference` と `operations` を更新可能な形で着地できる。

## リスクとトレードオフ

- 実行経路が増えるため、初期は運用理解コストが上がる。
- Rust native と WASM の両方を持つと、adapter ごとの差分管理が必要になる。
- metrics-first を進めると、後段で自由に field を拾う方式より実装が増える。
- `Interactive` と `HeadlessMetrics` の分離が不十分だと、実装が二重化しやすい。

ただし、次の利点が大きい。

- 日常の待ち時間を削減できる
- 故障点の切り分けがしやすい
- transport/UI 都合を simulation core から外しやすい
- perf 改善の対象を「モデル計算」と「観測同期」に分けて追える

## 実施計画

1. 本 proposal を追加する。
2. `docs/operations/test.md` に検証ジョブの層を追記する。
3. Rust native の `seed regression` runner を最小構成で導入する。
4. 既存 `seed regression` スクリプトと同じ baseline / threshold ルールで比較できるようにする。
5. `VerificationMode` の初版を導入し、`HeadlessMetrics` では UI 向け観測を無効化する。
6. module diagnostics を `perf` と `regression` の両方で使える形に整理する。
7. `pending_post_step` の扱いを文書化し、phase model への統合可否を別 proposal または decision で詰める。
8. 実装後、`reference` 文書と `operations` 文書を更新する。

## 未解決事項

- `verification runtime` を別 crate とするか、`application` 配下の module として持つか。
- baseline フォーマットを既存 JSON と共通化するか、Rust native 専用形式を許容するか。
- `perf gate` を native / wasm / worker のどこまで分けるか。
- `history snapshot` を `Interactive` 専用にするか、回帰モードでも限定的に持つか。
- `HeadlessMetrics` でどの metrics を正本とするか。
- scientific benchmark 実行系を同じ runner に載せるか、完全に別系統にするか。
