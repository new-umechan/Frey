# Exner系 sediment 収支と明示的沈降による陸化ドリフト抑制

## Status

Accepted

## 背景

- 現行の未コミット対応では、`Geology` 最終反映後に全球一様 freeboard シフトを入れて海陸比を戻していた。
- この方法は海陸比の発散抑制には効くが、侵食・堆積の質量収支そのものは改善しない。
- 実装上、`Hydrology` が計算する `deposition_rate` が `erosion_rate` 総量を超えても、そのまま `height` に反映されうる。
- 既定値では `tectonic_subsidence_gain` と `thermal_subsidence_gain` が 0 で、長期平均として沈降側の復元力が弱い。

## 目的

- 全陸化ドリフトを、後段の一様オフセットではなく、侵食・堆積・沈降の側で抑える。
- fluvial sediment budget を少なくとも tick 単位では非発散にする。
- 水系の depression handling は既存の fill-spill を活かしつつ、地形更新側の質量収支を優先して整える。

## 提案概要

- `Geology` の hydrology 反映では、`deposition_rate` 総量を `erosion_rate` 総量と
  mobile sediment inventory の範囲内へ一様スケールする。
- これにより、fluvial 成分については Exner 的に `Σdeposition <= Σerosion` を満たす。
- `Σerosion > Σdeposition` の差分は、未解像の深海・海盆への輸送として open boundary export とみなす。
- glacial erosion は現段階では独立 transport を持たないため、deposition 原資には数えず export 扱いにする。
- `Geology` の内生的な鉛直変位（tectonics / diffusion / isostasy）は、1 tick ごとの全球平均変位が 0 になるよう拘束する。
- 既定パラメータとして、`tectonic_subsidence_gain` と `thermal_subsidence_gain` を小さく正値へ戻す。
- `target_sea_ratio` は初期地形生成の拘束として維持し、runtime では後段の全球 terrain shift を行わない。
- 動的海面は `sea_level_offset` に集約し、runtime では地形ではなく海面オフセット側を弱く緩和する。

## スコープ

- `rust/src/sim/exec/geology.rs`
- `config/terrain.yaml`
- 関連 reference / decision 文書

## 成功条件

- `seed_regression --seeds alpha --ticks 250 --level 6` で全陸化しない。
- `deposition_rate` 総量が `erosion_rate` 総量を恒常的に上回って陸面積を押し上げる挙動を止める。
- `height_std` が急激に縮退しない。

## リスクとトレードオフ

- deposition を一様スケールするため、局所の堆積場再現性は簡略化される。
- ただし、全球一様標高シフトよりは sediment budget に直接作用し、解釈しやすい。
- 内生鉛直変位の零平均拘束は basin ごとの厳密保存ではないが、少なくとも「内部力だけで全球平均標高が単調ドリフトする」破綻を抑えられる。
- glacial sediment を export 扱いにしているため、氷河起源の堆積地形まではまだ再現しない。
- subsidence 既定値は較正途中であり、将来的には観測制約や benchmark で再較正が必要。

## 実施計画

1. freeboard 一様シフトを撤去する
2. fluvial deposition に Exner 系の総量制約を入れる
3. 小さな tectonic / thermal subsidence を既定値に戻す
4. runtime の全球 terrain shift を撤去する
5. reference / decision を更新する
6. seed regression と unit test で確認する

## 未解決事項

- deposition 制約を全球一様ではなく basin ごと・sink ごとに行うか
- glacial sediment の明示的 transport / storage を追加するか
- water storage も public state に持ち、water budget diagnostics を常設するか
- `sea_level_offset` へ海面判定をどこまで統一するか

## 参考

- An et al., 2018, flux / entrainment form of Exner sediment conservation
- Barnes et al., 2021, Fill-Spill-Merge depression hierarchy
- Campforts et al., 2020, HyLands / SPACE による sediment mass conservation
