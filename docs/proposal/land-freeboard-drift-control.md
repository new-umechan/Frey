# 全陸化ドリフト抑制と Freeboard 制御

## Status

Superseded

Replaced by: `docs/proposal/sediment-mass-conserving-land-balance.md`

## 背景

- `seed_regression` の `alpha`（level=6）で、tick進行とともに陸面積が単調増加し、`tick=250` で `land_cells=40962/40962`（全陸化）に達する。
- 初期地形生成では `target_sea_ratio` を使って海面基準を決めるが、ランタイムの地形進化では同等の拘束が弱く、平均高度の正ドリフトに対して復元力が不足している。
- 既定値で `tectonic_subsidence_gain` と `thermal_subsidence_gain` が 0 のため、隆起側バイアスの可能性がある。

## 目的

- 長期tickで海陸比が破綻して全陸化する挙動を止める。
- 既存の「海面=0」前提（Hydrologyの一部ロジック）と整合した形で、地形自由度を保ちつつ freeboard を安定化する。

## 提案概要

- 当初案では、`Geology` 最終反映の後段で `target_sea_ratio` に基づく **全球一様オフセット補正** を導入する想定だった。
- その後、この案は `sea_level_offset` を使う既存 runtime と海面の意味が二重化しやすいことが分かり、撤回した。
- 現在は `docs/proposal/sediment-mass-conserving-land-balance.md` の Exner 系収支制約と明示的沈降を正本とする。

## スコープ

- `rust/src/sim/exec/geology.rs` の地形最終反映パス

## 成功条件

- `seed_regression --seeds alpha --ticks 250 --level 6` で全陸化しない。
- `land_cells` が `cell_count` に張り付かない。
- `height_std` が極端に縮退しない（地形起伏の全消失を避ける）。

## リスクとトレードオフ

- 全球一様シフトは「絶対標高」を直接操作するため、物理量の補正ではなく観測量の再中心化になりやすい。
- `sea_level_offset` と併用すると、海面を地形側と海面側の両方で動かすことになり、因果解釈が悪化する。
- そのため、将来は Exner 収支・flexural isostasy・`sea_level_offset` への統一で扱う方向が望ましい。

## 実施計画

1. freeboard 漸近補正を導入
2. 代表seedで 1/25/50/100/150/200/250 tick を確認
3. subsidence 既定値変更は別途較正案として起票

## 未解決事項

- `sea_level_offset` を動的海面として全面採用するか（現状はモジュールごとの閾値利用が混在）
- 沈降項の較正（観測拘束・ベンチ指標との両立）
