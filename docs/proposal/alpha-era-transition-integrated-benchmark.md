# Alpha Era Transition Integrated Benchmark

## Status

Accepted

## 背景

- `alpha` の手動確認で、`Environment` 期の数 tick 内に陸消失/海消失へ偏る症状が断続的に観測された。
- 個別 module の solo bench では、phase 境界をまたいだ統合挙動（`Geology` + `Glaciology` + `Hydrology`）の急変を捕まえにくい。
- `test` 系 gate は短尺比較が中心で、`t=800` 近傍の連続時系列異常を benchmark artifact として残しにくい。

## 目的

- `alpha` 固定で `Crust -> Environment` 遷移近傍の統合挙動を benchmark として定常監視する。
- 手動目視で見ていた「急変」を、同じ指標・同じ窓で再現可能な JSONL artifact と FAIL 条件へ落とす。

## 提案

- `rust/src/bin/alpha_transition_guard.rs` を追加する。
- 既定で `tick=0..900` を実行し、`780..900` の各 tick を JSONL に出力する。
- 記録指標:
  - `land_cells`
  - `land_ratio`
  - `sea_level_offset`
  - `ocean_water_inventory_drift`
  - `ice_inventory`
- 異常判定（既定値。env で上書き可能）を二段階化する:
  - hard fail:
    - `|mass_proxy_drift|` 超過（`mass_proxy = ocean_water_inventory + sea_level_coupling * ice_inventory`）
    - 非有限値（`NaN`/`Inf`）
  - warning:
  - `land_ratio` が `[0.15, 0.85]` を外れる
  - `|Δland_ratio| > 0.03 / tick`
  - `|Δsea_level_offset| > 0.08 / tick`
  - `|ocean_water_inventory_drift| > 1e-4`
  - 遷移前窓（`tick<=799`）と遷移後窓（`800..840`）での `median(land_ratio)` 差が `0.04` 超過
  - 遷移前窓（`tick<=799`）と遷移後窓（`800..840`）での `median(sea_level_offset)` 差が `0.10` 超過
- hard fail 条件で benchmark を失敗終了し、warning は artifact のみ記録する。

## スコープ

- `rust/src/bin/alpha_transition_guard.rs`
- `package.json`
- `docs/operations/bench/glaciology/alpha_transition_guard.md`

## 成功条件

- `pnpm bench:alpha:transition` で benchmark が実行できる。
- `benches/results/alpha_transition_guard/alpha_transition_guard.jsonl` に時系列 artifact が追記される。
- 異常ケースで benchmark が非 0 終了する。
