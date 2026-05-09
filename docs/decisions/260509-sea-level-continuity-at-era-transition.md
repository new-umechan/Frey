# Era遷移時の海面連続性を境界再基準化で担保する

## Status

Accepted

## Context

- `ocean_water_inventory` は world 初期化時に推定されるが、`Crust` 期の地形変化に追随して再同定されない。
- 既存実装では `Crust` 期の `Glaciology` が早期 return し、`sea_level_offset` の inventory-based closure も止まる。
- その結果、`Environment` 期に入った最初の `Glaciology` tick で `sea_level_offset` が大きく更新され、
  海陸判定が不連続に変化して全面海化に近い挙動が生じうる。

## Decision

- `Crust` 期は海面変数を動かさず、地形基準（freeboard）で planet-building を進める。
- `Crust` 期の `Glaciology` では、氷床成長・融解・氷床侵食・アイソスタシー更新は実行しない。
- `Crust`→`Environment` 遷移時に、その時点の `height` と `sea_level_offset` から
  `ocean_water_inventory` を再計算し、`ocean_water_inventory_baseline` にも同値を設定する。
- `Environment` 以降は、再基準化した inventory を使って mass-based sea-level closure を更新する。

## Rationale

- 海面ジャンプの直接原因は、`Crust` 期地形と `Environment` 期 inventory の不整合であり、
  境界再基準化で初回 closure ショックを除去できる。
- `height`（固体地形）と `sea_level_offset`（海面）の役割分離を維持したまま、海水量固定の質量保存解釈を守れる。
- 氷床物理を `Crust` 期へ持ち込まず、`Environment` 入り口で inventory を整合させることで
  phase separation と連続性を両立できる。

## Consequences

- `Crust`→`Environment` 境界での `sea_level_offset` 段差が縮小し、海陸セル比の急変が抑制される。
- `Crust` 期は海面固定、`Environment` 期以降は mass-based closure という責務分離が明確になる。
- 将来、厳密な reservoir accounting（陸上貯留を含む）へ進む場合でも、
  「不変量を保って海面を連続解で更新する」枠組みは再利用できる。
