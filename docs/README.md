# 仕様と実装の対応

Freyの設計文書と、現在の実装境界を対応づけるための索引。
文書とコードの責務がずれたときは、このファイルを先に更新する。

## 読み始める順番

1. `docs/architecture/overview.md`
2. `docs/architecture/module_boundaries.md`
3. `docs/architecture/data_model.md`
4. `docs/architecture/phase_control.md`
5. `docs/interface/wasm_api.md`
6. `docs/interface/ui_spec.md`
7. `docs/manage/test.md`
8. `docs/manage/benchmark.md`

## 調査メモ

- `docs/search/search.md`
  - プレート、気候、河川侵食まわりのラフな調査メモ
- `docs/search/gospl_sink_model.md`
  - goSPLを参照したSink容量モデルの要点と、本プロジェクトへの適用方針

## 実装マップ

### フロントエンド

- `web/src/main.js`
  - 起動エントリーポイント
- `web/src/app/`
  - アプリの組み立て、UI状態、WASM同期
  - `app.js`: 依存の組み立てとアプリ起動
  - `world-sync.js`: WASM応答をアプリ状態へ同期
  - `world-loop.js`: tick進行と進行状態リセット
  - `era-presets.js`: 時代プリセット表示と変換
  - `plate-hover.js`: プレートhover表示
  - `terrain-renderer.js`: 地形属性の描画反映
- `web/src/gfx/`
  - Three.js描画、カメラ、地形ビジュアル
- `web/src/ui/`
  - DOM取得とイベント配線
- `web/src/interface/`
  - UIとWASMの境界
- `web/src/app/runtime/`
  - フロントエンド側のランタイム状態
- `web/src/app/debug/`
  - デバッグ補助

### Rust / WASM

- `rust/src/lib.rs`
  - 公開エントリーポイント
- `rust/src/wasm_api/world_sim/`
  - `WorldSimController` のWASM境界
  - `mod.rs`: コントローラ本体
  - `api/worlds.rs`: world生成と進行
  - `api/queries.rs`: 観測API
  - `api/commands.rs`: 介入、fork、checkpoint
  - `types.rs`: JSとの送受信型
  - `state.rs`: 管理中ワールドと履歴
  - `helpers.rs`: サンプリング、履歴管理、侵食状態同期
- `rust/src/sim/`
  - `World` とtick進行
  - `erosion.rs`: 侵食オートマトン状態
  - `step.rs`: Execのオーケストレーション
  - `step/terrain.rs`: 地質進行の束ね
  - `step/boundary_dynamics.rs`: 境界分類とプレート運動
  - `step/surface_dynamics.rs`: 応力から地表更新
  - `step/river.rs`: 河川と侵食オートマトン接続
  - `step/geology.rs`: Geology全体の束ね
- `rust/src/sim/geology_types.rs`
  - 地形生成の公開型
  - `GeologyParams`、`GeologyOutput`、`MeshOutput`
- `rust/src/sim/terrain/`
  - 地形生成ドメイン
  - `terrain.rs` は `noise`、`plates`、`boundaries`、`surface`、`pipeline` を束ねる
  - `plates/`、`boundaries/`、`surface/` 配下は大物ファイルを責務単位で細分化した内部実装

## 文書とコードの対応

### `docs/architecture/overview.md`

- 対応コード:
  - `rust/src/sim/world.rs`
  - `rust/src/sim/step.rs`
  - `web/src/app/app.js`

### `docs/architecture/module_boundaries.md`

- 対応コード:
  - `rust/src/sim/step.rs`
  - `rust/src/sim/step/`
  - `rust/src/sim/world.rs`

### `docs/architecture/data_model.md`

- 対応コード:
  - `rust/src/sim/world.rs`
  - `web/src/app/runtime/state.js`
  - 役割: `World State` / `Exec State` / `Graph State` の配置と責務を定義

### `docs/interface/wasm_api.md`

- 対応コード:
  - `rust/src/wasm_api/world_sim/`
  - `web/src/interface/wasm.js`
  - `web/src/app/world-sync.js`

### `docs/interface/ui_spec.md`

- 対応コード:
  - `web/src/app/`
  - `web/src/ui/`
  - `web/src/gfx/`

## 更新ルール

- 500行超のファイルは優先分割対象として扱う
- 500行未満でも複数責務があるファイルは分割する
- 実装の責務名を変えたら、同じ変更でこの索引も更新する
- 存在しない文書や未実装前提の索引は残さない
