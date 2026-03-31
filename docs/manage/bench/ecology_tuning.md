# Ecology パラメータチューニングガイド

## 概要

`bench:tune:ecology` コマンドは、Ecology 単体ベンチマークのスコアを最大化するパラメータを自動探索します。

## 基本的な使い方

```bash
# 基本的な実行（引数なし）
pnpm run bench:tune:ecology

# カスタム設定で実行（直接 Python から推奨）
python3 benches/scripts/tune-ecology-params.py --max-runs 50
python3 benches/scripts/tune-ecology-params.py --min-tree-rho 0.50 --min-ground-rho 0.35 --min-biome-f1 0.30
```

**注意**: pnpm 経由で引数を渡すと `--` が二重になる問題があるため、引数を使う場合は直接 Python スクリプトを実行してください。

## オプション

| オプション | 説明 | デフォルト値 |
|---|---|---|
| `--rust-path` | Ecology 実装ファイルのパス | `rust/src/sim/ecology/mod.rs` |
| `--output` | 結果出力先 JSONL ファイル | `benches/results/ecology_tuning/runs/ecology_tuning_runs.jsonl` |
| `--min-tree-rho` | 樹木被覆相関の最小制約値 | `0.30` |
| `--min-ground-rho` | 地被相関の最小制約値 | `0.20` |
| `--min-biome-f1` | バイオーム F1 スコアの最小制約値 | `0.10` |
| `--max-runs` | 最大試行回数（0 で全組み合わせ） | `0` |
| `--grid-json` | カスタムグリッド定義 JSON ファイル | - |

**注意**: ecology_solo ベンチでは、`tree_cover rho` は 0.5-0.7 程度、`ground_cover rho` は 0.4-0.6 程度、`biome macro_f1` は 0.3-0.5 程度が現状の値です。制約は「ベースラインからどれだけ改善するか」を見るために緩めに設定しています。

## チューニング対象パラメータ

デフォルトで以下のパラメータを探索します：

### 樹木被覆ダイナミクス

- `tree_growth_rate`: 樹木の成長率（デフォルト：0.18）
- `tree_decline_rate`: 樹木の減少率（デフォルト：0.08）

### 地被ダイナミクス

- `ground_growth_rate`: 地被の成長率（デフォルト：0.16）
- `ground_decline_rate`: 地被の減少率（デフォルト：0.08）

### 撹乱ダイナミクス

- `disturbance_up_rate`: 撹乱増加率（デフォルト：0.22）
- `disturbance_down_rate`: 撹乱減少率（デフォルト：0.10）

### バイオーム分類閾値

- `alpine_threshold`: 高山帯の高度閾値（デフォルト：0.72）
- `tundra_threshold`: ツンドラの温度閾値（デフォルト：-2.5°C）
- `desert_threshold`: 砂漠の降水量閾値（デフォルト：220.0mm）
- `wetland_threshold`: 湿地の洪水閾値（デフォルト：0.58）
- `wetland_tree_threshold`: 湿地樹木閾値（デフォルト：0.55）
- `tropical_temp_threshold`: 熱帯温度閾値（デフォルト：22.0°C）
- `boreal_temp_threshold`: 北方林温度閾値（デフォルト：6.0°C）
- `forest_threshold`: 森林閾値（デフォルト：0.58）

## 事前準備

1. **地形データの準備**
   ```bash
   pnpm bench:dump-centroids
   pnpm bench:resample:terrain
   ```

2. **気候データの準備**
   ```bash
   pnpm bench:resample:climate
   ```

3. **Hydrology 入力・参照データの準備**
   ```bash
   pnpm bench:resample:hydro-input
   pnpm bench:resample:hydro-ref
   ```

4. **Ecology 参照データの準備**
   ```bash
   pnpm bench:resample:ecology-ref
   # または土壌データを含むバージョン
   pnpm bench:resample:ecology-ref:with-soil
   ```

5. **ベースライン実行**
   ```bash
   pnpm run bench --suite ecology_solo
   ```

## 結果の見方

### 標準出力

```json
{
  "search_space_size": 6561,
  "evaluated_runs": 100,
  "baseline": {
    "tree_cover": 0.612,
    "ground_cover": 0.485,
    "biome_macro_f1": 0.423
  },
  "constraints": {
    "min_tree_rho": 0.30,
    "min_ground_rho": 0.20,
    "min_biome_f1": 0.10,
    "baseline_tree_rho": 0.612,
    "baseline_ground_rho": 0.485,
    "baseline_biome_f1": 0.423
  },
  "best": {
    "trial": 42,
    "values": {
      "tree_growth_rate": 0.22,
      "tree_decline_rate": 0.06,
      "ground_growth_rate": 0.18,
      ...
    },
    "metrics": {
      "tree_cover": 0.638,
      "ground_cover": 0.512,
      "biome_macro_f1": 0.445
    },
    "objective_score": 0.542
  }
}
```

### 出力 JSONL

各行に試行結果が記録されます：

```json
{"trial": 1, "values": {...}, "metrics": {...}, "feasible": true, "objective_score": 0.512, "elapsed_sec": 52.3}
{"trial": 2, "values": {...}, "metrics": {...}, "feasible": false, "objective_score": -inf, "elapsed_sec": 48.7}
```

### 評価指標

- **tree_cover rho**: 樹木被覆の空間分布の相関（Spearman）
- **ground_cover rho**: 地被の空間分布の相関（Spearman）
- **biome_macro_f1**: バイオーム分類の Macro F1 スコア
- **biome_accuracy**: バイオーム分類の精度

### 目的関数

最適化の目的関数は以下の重み付き和です：

```
objective = 0.35 * tree_cover + 0.25 * ground_cover + 0.40 * biome_macro_f1
```

## カスタムグリッド定義

特定のパラメータに絞って探索したい場合：

```json
{
  "tree_growth_rate": [0.16, 0.18, 0.20, 0.22],
  "tree_decline_rate": [0.06, 0.08, 0.10],
  "forest_threshold": [0.54, 0.58, 0.62]
}
```

これを `ecology_grid.json` として保存：

```bash
python3 benches/scripts/tune-ecology-params.py --grid-json ecology_grid.json --max-runs 100
```

## 推奨ワークフロー

1. **ベースライン測定**: 現在のパラメータでベンチマーク実行
2. **広域探索**: `--max-runs 50` で大まかに最適値を探索
3. **局所探索**: 良さそうな値の周辺で細かいグリッドを定義
4. **検証**: 最適パラメータで複数回実行して安定性確認

## 注意事項

- **実行時間**: 1 試行あたり 45-90 秒程度（環境依存）
- **WASM ビルド**: パラメータ変更ごとに `terrain:sync` が実行されます
- **制約条件**: `min-tree-rho`、`min-ground-rho`、`min-biome-f1` を下回る設定は「実行不可」として除外されます
- **Rust 書き換え**: チューニングスクリプトは `rust/src/sim/ecology/mod.rs` の定数を書き換えます。終了時に元に戻りますが、異常終了した場合は手動での復元が必要です

## 関連コマンド

- `pnpm run bench --suite ecology_solo`: 単体ベンチマーク実行
- `pnpm run bench:run:ecology-series`: 複数回実行（準備中）
- `pnpm run bench:compare:ecology`: スコア比較（準備中）
- `pnpm run bench:check:ecology-quality`: 品質ゲート確認（準備中）

## 関連ファイル

- `rust/src/sim/ecology/mod.rs`: Ecology 実装
- `rust/benches/ecology_solo.rs`: Ecology 単体ベンチマーク
- `benches/data/ecology_ref.bin`: Ecology 参照データ
- `benches/results/ecology_main_scores.jsonl`: スコア記録
