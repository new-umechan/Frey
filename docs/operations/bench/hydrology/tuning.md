# Hydrology パラメータチューニングガイド

## 概要

`bench:tune:hydrology` コマンドは、Hydrology 単体ベンチマークのスコアを最大化するパラメータを自動探索します。

## 基本的な使い方

```bash
# 基本的な実行（引数なし）
pnpm run bench:tune:hydrology

# カスタム設定で実行（直接 Python から推奨）
python3 benches/scripts/tune-hydrology-params.py --max-runs 50
python3 benches/scripts/tune-hydrology-params.py --min-flow-rho 0.70 --min-lake-f1 0.45
```

**注意**: pnpm 経由で引数を渡すと `--` が二重になる問題があるため、引数を使う場合は直接 Python スクリプトを実行してください。

## オプション

| オプション       | 説明                               | デフォルト値                                                        |
| ---------------- | ---------------------------------- | ------------------------------------------------------------------- |
| `--config-path`  | パラメータ設定ファイルのパス       | `config/terrain.yaml`                                               |
| `--output`       | 結果出力先 JSONL ファイル          | `benches/results/hydrology_tuning/runs/hydrology_tuning_runs.jsonl` |
| `--min-flow-rho` | 河川流量相関の最小制約値           | `0.10`                                                              |
| `--min-lake-f1`  | 湖検出 F1 スコアの最小制約値       | `0.0`                                                               |
| `--max-runs`     | 最大試行回数（0 で全組み合わせ）   | `0`                                                                 |
| `--grid-json`    | カスタムグリッド定義 JSON ファイル | -                                                                   |

**注意**: 1 tick ベンチマークでは、`river_flow rho` は 0.1-0.3 程度、`lake_f1` は 0.0-0.1 程度が現状の値です。制約は「ベースラインからどれだけ改善するか」を見るために緩めに設定しています。

## チューニング対象パラメータ

デフォルトで以下のパラメータを探索します：

### 河川ネットワーク形成

- `river_accumulation_threshold`: 河川として認識される最小流量閾値
- `river_inertia_gain`: 流路の慣性（前回からの維持）
- `river_curvature_penalty`: 曲がり具合へのペナルティ

### 湖・Sink パラメータ

- `sink_local_rebuild_radius`: Sink 再構築の局所半径
- `sink_overflow_hysteresis`: オーバーフローのヒステリシス
- `sink_min_capacity`: 最小 Sink 容量

### 基底流パラメータ

- `baseflow_infiltration_rate`: 浸透率
- `baseflow_release_rate`: 基底流放出率
- `baseflow_storage_cap`: 地下水貯留容量

### 侵食・堆積パラメータ

- `hydraulic_erosion_rate`: 水力侵食率
- `hydraulic_deposit_rate`: 水力堆積率
- `sediment_capacity_gain`: 堆積物容量係数

## 事前準備

1. **地形データの準備**

    ```bash
    pnpm bench:dump-centroids
    pnpm bench:resample:terrain
    ```

2. **Hydrology 入力データの準備**

    ```bash
    pnpm bench:resample:hydro-input
    pnpm bench:resample:hydro-ref
    ```

3. **ベースライン実行**

    ```bash
    pnpm run bench --suite hydrology_solo
    ```

## 結果の見方

### 標準出力

```json
{
  "search_space_size": 19683,
  "evaluated_runs": 100,
  "baseline": {
    "river_flow": 0.741,
    "lake_f1": 0.501
  },
  "constraints": {
    "min_flow_rho": 0.65,
    "min_lake_f1": 0.40,
    "baseline_flow_rho": 0.741,
    "baseline_lake_f1": 0.501
  },
  "best": {
    "trial": 42,
    "values": {
      "river_accumulation_threshold": 0.012,
      "river_inertia_gain": 0.25,
      ...
    },
    "metrics": {
      "river_flow": 0.768,
      "lake_f1": 0.523
    },
    "objective_score": 0.650
  }
}
```

### 出力 JSONL

各行に試行結果が記録されます：

```json
{"trial": 1, "values": {...}, "metrics": {...}, "feasible": true, "objective_score": 0.612, "elapsed_sec": 45.3}
{"trial": 2, "values": {...}, "metrics": {...}, "feasible": false, "objective_score": -inf, "elapsed_sec": 42.1}
```

## カスタムグリッド定義

特定のパラメータに絞って探索したい場合：

```json
{
    "river_accumulation_threshold": [0.008, 0.01, 0.012, 0.014, 0.016],
    "river_inertia_gain": [0.2, 0.25, 0.3],
    "baseflow_storage_cap": [200.0, 240.0, 280.0]
}
```

これを `hydro_grid.json` として保存：

```bash
python3 benches/scripts/tune-hydrology-params.py --grid-json hydro_grid.json --max-runs 100
```

## 推奨ワークフロー

1. **ベースライン測定**: 現在のパラメータでベンチマーク実行
2. **広域探索**: `--max-runs 50` で大まかに最適値を探索
3. **局所探索**: 良さそうな値の周辺で細かいグリッドを定義
4. **検証**: 最適パラメータで複数回実行して安定性確認

## 注意事項

- **実行時間**: 1 試行あたり 30-60 秒程度（環境依存）
- **WASM ビルド**: パラメータ変更ごとに `config:sync`（統合同期）が実行されます
- **制約条件**: `min-flow-rho` と `min-lake-f1` を下回る設定は「実行不可」として除外されます

## 関連コマンド

- `pnpm run bench --suite hydrology_solo`: 単体ベンチマーク実行
- `pnpm run bench:run:hydrology-series`: 複数回実行
- `pnpm run bench:compare:hydrology`: スコア比較
- `pnpm run bench:check:hydrology-quality`: 品質ゲート確認
