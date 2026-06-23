# Damage-first plate emergence initialization

## Status

Superseded

Superseded by `260619-regime-based-plate-emergence-v1.md`.

## Context

現在の初期プレート分割は、球面上の additively weighted Voronoi で `plate_id` を直接生成する。
これは実装と検証が単純だが、Frey の地質史としては「最初から完成済みの plate がある」前提になる。

初期地球のリソスフェアは、硬い蓋がマントル対流、プルーム、冷却、密度不安定、物性差によって局所的に破壊され、
再利用される弱線から後に plate-like な剛体領域が現れたと考える方が説明しやすい。
Frey ではこれを厳密な熱機械シミュレーションではなく、生成則として近似する。

## Decision

初期 `plate_id` は直接置かず、pre-plate damage field から抽出する。

MVP の一時 field は次とする。

- `strength`: 壊れにくさ
- `stress`: 現在かかっている破壊圧
- `damage`: 破壊履歴と境界化しやすさ

初期化は以下の順序で行う。

1. `phi`、低周波ノイズ、近傍 contrast から温度、厚さ、material contrast、plume/downwelling 近似を作る
2. `strength` と `stress` を計算する
3. `stress > strength` の超過分で `damage` を 20-100 step 程度育てる
4. `damage` を軽く平滑化し、しきい値以上を boundary candidate とする
5. boundary 以外の connected components を plate nucleus として抽出する
6. 小さすぎる component と boundary cell を隣接する有効 plate へ吸収する
7. 有効な `plate_id` と初期運動属性を後段の plate simulation へ渡す

採用条件は次の近似とする。

```text
弱い境界 + 強い内部 = plate
```

実装上は既存の `PlateAttr` へ接続するため、抽出された plate ごとに平均位置、平均 plume、平均 downwelling、
craton-like な高強度セル比率を集計し、既存の速度・ドリフト軸へ反映する。
低周波ランダム流を主成分にし、plume から離れる成分、downwelling へ向かう成分、craton-like 領域の抵抗を副成分にする。

## Rationale

この方式では、境界タイプを初期条件として固定せず、damage network と後段の相対運動から ridge/trench/collision/transform を分類できる。
また、境界や山脈を「過去の弱線が再利用された結果」として説明しやすい。

このモデルは damage mechanics、lithospheric inherited weakness、plume-rift interaction を生成用に粗く近似するものであり、
連続体熱機械モデルの代替ではない。
pre-plate pass は初期傷を作るための短い初期化手続きとして扱う。

## Consequences

利点:

- plate を初期条件ではなく emergent region として扱える
- Frey の地質史説明と整合する
- 後段の境界分類と履歴場に接続しやすい

欠点:

- damage threshold と小領域 merge の調整が出力の見た目に強く効く
- 現時点の `strength/stress` は簡略化された proxy であり、物理量そのものではない
- 不安定な seed/param では有効 component が不足する可能性があるため、当面は power Voronoi fallback を残す

## Canonical Docs Updated

`docs/reference/modules/geology.md` の初期プレート分割仕様を、球面 power Voronoi から damage-first 初期化へ置き換えた。
