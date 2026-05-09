# 海面安定性と沿岸浸水ベンチの導入

## Status

Accepted

## 背景

- `alpha` seed の長期実行で、約 100 tick 時点までに `land_cells` がほぼ 0 になり、全面海化に近い挙動が出る。
- 現状の `seed_regression` は `land_cells` や `height_std` は監視しているが、`sea_level_offset` 自体を出しておらず、海面暴走と地形沈降を切り分けにくい。
- `ocean_water_inventory` は初期化時に決まり、その後の `Crust` 期地形変化には追随しない。
  この状態で `Environment` 期に入ると `Glaciology` の海面 closure が初回で大きく効き、`sea_level_offset` が不連続にジャンプしうる。
- 一方で Earth 条件の bench には「海面を 1m, 5m, 10m, 20m, 50m 上げたとき、沿岸低地がどれだけ沈むべきか」を見る指標がまだない。

## 目的

- 100 tick 前後の全面海化を、海面側の暴走か地形側の問題かで即座に判定できるようにする。
- `Crust`→`Environment` 境界で `sea_level_offset` が不連続ジャンプしないようにする。
- Earth 参照地形に対して、沿岸浸水応答が現実離れしていないかを定量比較できるようにする。

## 提案概要

- `seed_regression` / `WorldMetrics` に `sea_level_offset` を追加し、海面オフセットの長期ドリフトを回帰監視対象にする。
- `Crust` 期は海面固定で進め、`Environment` 期突入時に `ocean_water_inventory` を再基準化する。
- `Crust` 期では氷床成長・融解・侵食は実行しない。
- `Hydrology` の fluvial erosion / deposition による標高更新も `Crust` 期では地形へ反映しない。
  `Crust` 期の主因は tectonics とし、表面侵食は `Environment` 期以降へ移す。
- さらに `Crust` 期に限って、初期地形生成時の land ratio を保つ weak freeboard recentering を入れる。
  これは runtime 全期間の海陸比制御ではなく、planet-building phase の初期条件安定化として扱う。
- 地形起伏の縮退対策としては、`Crust` 固定の係数分岐ではなく、内生 forcing に対して
  拡散項とアイソスタシー調整が優勢になりすぎたときだけ平滑化を減衰させる
  `state-dependent smoothing limiter` を使う。
- `geology_solo` に Earth 参照地形ベースの `coastal inundation response` 診断を追加する。
  評価点は `+1m`, `+5m`, `+10m`, `+20m`, `+50m` とし、各海面上昇量での land ratio と、新規浸水 land ratio を
  generated terrain と terrain reference の両方で計算し、その差を記録する。

## スコープ

- `docs/operations/benchmark.md`
- `docs/operations/bench/geology/solo.md`
- `rust/src/sim/world/metrics.rs`
- `rust/src/bin/seed_regression.rs`
- `rust/src/sim/glaciology/surface.rs`
- `benches/rust/benches/geology_solo.rs`

## 成功条件

- `alpha` の 100 tick 前後で、`land_cells` だけでなく `sea_level_offset` の推移も追える。
- `Crust`→`Environment` 遷移で `sea_level_offset` の不連続ジャンプが抑制される。
- `geology_solo` の JSONL artifact に、`+1m/+5m/+10m/+20m/+50m` の浸水応答差分が残る。

## リスクとトレードオフ

- `Environment` 期突入時に inventory を再基準化するため、
  `Crust` 期を絶対基準にした海水量比較はできなくなる。
  ただし遷移境界の段差による全面海化を避け、phase separation は明確になる。
- `+50m` 応答は高解像 coastline を再現するものではなく、`mesh_level=6` の粗い hypsometry 診断に留まる。
- Earth 浸水応答 bench は幾何学的な妥当性比較であり、氷床・GIA・dynamic topography の完全な再現を保証するものではない。

## 実施計画

1. `sea_level_offset` を metrics / regression 出力へ追加する
2. `Crust`→`Environment` 境界で ocean inventory を再基準化する
3. `geology_solo` に浸水応答診断を追加する
4. benchmark / proposal 文書を更新する
5. seed regression と geology bench を再実行して確認する

## 未解決事項

- `Environment` 境界再基準化後の海面ドリフト許容幅をどの程度 gate 化するか
- Earth 浸水応答を diagnostics 止まりではなく将来 gate 化するか
- `sea_level_offset` の絶対値に対して、許容上限をメートル換算で持つか
