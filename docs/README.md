# 仕様の全体像

Freyの仕様メモと設計ドキュメントをまとめる場所。

## 読み始める順番

1. `docs/architecture/overview.md`
2. `docs/architecture/phase_control.md`
3. `docs/architecture/data_model.md`
4. `docs/core/` 配下の各仕様
5. `docs/interface/` 配下の各仕様

### 世界地形シミュレーション再設計（4点セット）

1. `docs/core/redesign/README.md`
2. `docs/core/redesign/01_common_foundation.md`
3. `docs/core/redesign/02_plate_rigid_rotation.md`
4. `docs/core/redesign/03_vertical_velocity_u.md`
5. `docs/core/redesign/04_topography_evolution_equation.md`
6. `docs/core/redesign/05_erosion_stream_power_diffusion.md`
7. `docs/core/redesign/06_climate_precip_k_modulation.md`
8. `docs/core/redesign/07_coupling_execution.md`
9. `docs/core/redesign/08_calibration_validation.md`

## ディレクトリ構成

### `config/`

実行時のパラメータファイルを置く。

- `terrain.yaml`: 地形生成と河川侵食の生成系パラメータ
- `runtime.yaml`: 時代制御・活動量観測などランタイム挙動の調整パラメータ

### `docs/architecture/`

全体構成、時代スケール制御、Worldの責務など、上位設計を置く。

- `overview.md`: 全体像、時代の考え方、並列進行の方針
- `phase_control.md`: 時代スケール制御、遷移、更新比率の方針
- `data_model.md`: `World` / `core` / `layers` などの構造メモ

### `docs/core/`

地形・河川・気候など、世界の基盤計算系の仕様を置く。

- `plate.md`: プレート/地形生成
- `hydrology.md`: 河川・侵食・堆積
- `climate.md`: 気候（気温・降水）
- `ecology.md`: 生態（可住性・一次生産など）

### `docs/interface/`

UIとWASM APIなど、外部との接続仕様を置く。

- `ui_spec.md`: UI仕様
- `wasm_api.md`: WASM公開API仕様

### `docs/manage/`

運用メモ、TODO、テスト観点などの管理用ドキュメントを置く。

- `test.md`: テスト方針・確認メモ
- `todo.md`: タスク管理メモ

## 書き分けルール

- `architecture`: なぜその構成にするか（責務分割、依存関係、時代制御）
- `core`: 何を計算するか（入力、状態量、更新則）
- `interface`: 外からどう使うか（UI/API）
- `manage`: 作業メモ、運用メモ

## 注意

- 仕様は段階的に更新する。未確定事項は削除せず、未決として残す。
