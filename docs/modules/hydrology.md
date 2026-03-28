# Hydrologyの詳細仕様

## 目的

Hydrologyは、地形と気候から流路・流量・侵食・堆積を計算する。
Systemは2つに分かれる。

- `HydrologyMFDSystem`：流路の再計算と流量の初期集積
- `HydrologyFlowSystem`：流路固定後の流量更新のみ

毎tickで次の値を`CellStore`へ書く。

- 流路（流下先・分配率）
- 流量
- 侵食量
- 堆積量
- 河川輸送コスト
- 湖フラグ

HydrologyはCellStoreとClockだけを読む。

## System構成と実行条件

| System | 実行条件 |
| --- | --- |
| `HydrologyMFDSystem` | 地殻形成期・環境形成期は毎tick実行。先史期以降はExecSystemが地形変化フラグを検知したtickのみ実行 |
| `HydrologyFlowSystem` | 先史期以降、毎tick実行 |

地形変化フラグの判定はExecSystemが担う。
GeologyはCellStoreに標高を書くだけであり、フラグ管理はしない。

## 入力

Hydrologyが読む主な値は次のとおり。

- `geology.height`
- `climate.runoff`
- FeedbackQueue（`Subsistence`・`Settlement` による取水・ダム）

## 出力

Hydrologyは次の配列を全セル分持つ。

- `hydrology.river_downstream`
- `hydrology.river_flow`
- `hydrology.erosion_rate`
- `hydrology.deposition_rate`
- `hydrology.river_transport_cost`
- `hydrology.is_lake`

`river_upstream` は保持しない。
流域の塗り分けが必要になった時点で再検討する。

## データ構造

```rust
river_downstream: Vec<SmallVec<[(CellId, f32); 3]>>,
is_lake:          Vec<bool>,
```

`river_downstream` の各要素は `(流下先CellId, 分配率f32)` のペアである。
1セルあたりの流下先は通常1〜3個程度であり、SmallVecの内部バッファサイズを3とする。

## MFDモデル（HydrologyMFDSystem）

### 流路計算の方針

Multiple Flow Direction（MFD）を採用する。
SFDは扇状地・平原・デルタの面的な広がりを表現できないため採用しない。

流下先への分配率は勾配のべき乗で決まる。

```text
fraction_i ∝ slope_i ^ x
```

Holmgren指数 `x` は勾配の線形関数とする。

```text
x = a * slope + b
```

`a` は勾配への感度、`b` は緩斜面（デルタ）の分散度を決める。
具体的な値は実装時に調整する。

急斜面では `x` が大きくなり流路が集中する。
緩斜面・デルタでは `x` が小さくなり流路が分散する。

### 処理順序

1. 窪地を検出して `is_lake=true` を立てる
2. 湖セルは隣接セルの中で最も低い鞍部を流下先として1本設定する
3. 通常セルはMFDで分配計算する（`fraction ∝ slope ^ x`）
4. 上流から順に流量を積み上げる

### 発散対策

発散を防ぐための安全装置を2つ設ける。

1. 窪地の湖化
窪地（流下先が存在しないセル）は湖として扱い、流量をそこで吸収する。
湖セルは隣接セルの中で最も低い鞍部を唯一の流下先として設定し、溢れた水を流下させる。

2. 変化量クランプ
流量・侵食量・堆積量に上限を設け、1tick内の急激な変化を抑制する。
クランプ値は実装時に調整する。

## 侵食・堆積

侵食量と堆積量はHydrologyが計算してCellStoreに書く。
標高への最終反映はGeologyが行う。

## 河川輸送コスト

河川輸送コストはSettlementとTradeが読む。
流量と流路の勾配から計算する。

関連:

- `docs/architecture/module_boundaries.md`
- `docs/architecture/data_model.md`
- `docs/modules/climate.md`
