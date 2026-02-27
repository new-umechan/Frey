# Sim構成整理とRust移管計画

## 現在のsim構成

- `src/sim/runtime/`
  - `state.js`: ランタイム状態の初期化
  - `activity.js`: 活動度計算ユーティリティ
- `src/sim/terrain/`
  - `core-step.js`: 地形コア更新
  - `plate-motion.js`: プレート移動更新
  - `river-step.js`: 河川キュー・侵食反映
  - `generation/`: 地形生成の実行・変換・適用
- `src/sim/layers/`
  - `updates.js`: climate/ecology/civilization更新（WASMバンドル呼び出し）
- `src/sim/debug/`
  - `snapshot.js`: デバッグスナップショット

## Rust移管の進捗

### 完了

1. `apply_land_ratio_floor` をWASM化
- JSから呼び出し、失敗時はJSフォールバック

2. `climate`/`ecology`/`civilization` を一括APIへ移管
- `step_layers_bundle` を追加
- 1回のWASM呼び出しで3層を更新

### 次段階

1. `updateTerrainCoreStep` 本体のRust化
- 最も計算密度が高く、移管効果が大きい

2. `river-step`の一部WASM化
- 侵食後の反映パスをRust寄りへ移す

## 成功条件

- `main.js` は起動・ループ・接着のみ
- 高頻度の数値更新の大半がWASM側
- `npm run build` と既存挙動が一致
