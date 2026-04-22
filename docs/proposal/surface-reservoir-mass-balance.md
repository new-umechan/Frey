# 地表 reservoir 分離と mass-based sea level への移行

## Status

Accepted

## 背景

- `260422-exner-sediment-balance-and-subsidence` により、fluvial `deposition_rate` の総量制約、明示的沈降、`sea_level_offset` への寄せを導入した。
- これは「全陸化ドリフトを止める」中間策としては妥当だが、保存している量がまだ曖昧である。
- 現状は `solid Earth`、移動中の sediment、海水、氷が `height` の変化へ部分的に混ざって反映されており、何を保存しているかを説明しづらい。
- `Geology` の零平均拘束も、現時点では「内生過程だけで全球平均標高が単調ドリフトしないようにする」数値安定化としては有効だが、質量保存則そのものではない。

## 目的

- 保存対象を `height` ではなく reservoir の在庫量として定義し直す。
- 地形変化、海面変化、氷量変化を別変数として扱い、因果を分離する。
- 既存の Exner 的 budget 制約を、全球一様の近似から basin / export を持つ形へ引き上げる。
- 現行の零平均拘束を、最終仕様ではなく `temporary numerical closure for endogenous solid-surface drift` として位置づけ直す。
- 長期安定性の検証を、海陸比だけでなく観測量ベースの比較へ広げる。

## 提案概要

### 1. 保存する reservoir を分離する

少なくとも次を独立した在庫量として扱う。

- `solid_earth_mass`
- `mobile_sediment_mass`
- `marine_sediment_mass`
- `ocean_water_inventory`
- `ice_inventory`

`height` は一次状態ではなく、これらの在庫量と密度・面積・荷重応答から導出される結果とする。

初期段階では「各 reservoir を public / diagnostic state として明示する」ことを優先し、完全な力学結合は段階導入でよい。

### 2. Exner 系の収支を basin / export 境界まで明示する

現行の

- `Σdeposition <= Σerosion + mobile_sediment_budget`

は維持するが、位置づけを「全球一様近似」と明記する。

次段階では少なくとも次を導入する。

- basin / sink ごとの sediment in-out
- 海へ出た sediment を入れる `marine_sediment_reservoir`
- 氷河起源 sediment の独立 transport または export accounting

これにより、「どこで侵食され、どこに一時貯留され、どこから海へ出たか」を budget として追跡できるようにする。

### 3. `Geology` の内生鉛直変位を質量ベースへ移行する

最終形では、`tectonic uplift/subsidence`、`thermal subsidence`、`isostatic compensation` を平均標高の再中心化ではなく、
地殻厚・密度・荷重の変化として表す。

零平均拘束はただちに削除しない。
reservoir 分離と質量ベース診断が入るまでは、
`temporary numerical closure for endogenous solid-surface drift`
として残す。

ただし proposal の採用後は、その意味を「保存則」ではなく「暫定的な数値 closure」と明記する。

### 4. 海面を `sea_level_offset` に統一する

海陸比を合わせるために地形全体を動かすのではなく、海面は海面の変数として扱う。

海面は次の組み合わせから決める。

- `ocean_water_inventory`
- `ice_inventory`
- ocean basin capacity

`height` と `sea_level_offset` の役割を分離し、`land fraction` はその結果として決まる量にする。

### 5. 検証指標を観測量ベースへ拡張する

長期安定性の判定は「250 tick で全陸化しない」だけでは不十分である。

最低限、継続比較できる指標として次を持つ。

- `land fraction`
- hypsometric curve
- relief distribution
- river flux distribution
- basin / sink occupancy

可能なら reservoir diagnostics として次も追加する。

- global sediment export
- marine sediment accumulation
- ocean water / ice inventory drift
- endogenous solid-mass proxy drift

## スコープ

この proposal で決めること:

- 保存対象を reservoir として定義し直す方向
- v1 では `solid_earth_mass` を全球 diagnostic proxy として持ち、セル正本へはまだ分解しないこと
- v1 では `marine_sediment_mass` を一方向 sink として扱い、双方向交換は将来方向として別段階に送ること
- v1 では glacial sediment を fluvial transport に渡さず、source 記録と export / marine accounting に留めること
- v1 では `ocean basin capacity` を現地形から毎 tick 近似再計算すること
- 観測比較の正本 artifact を `benches/results/` に置くこと
- 零平均拘束の位置づけを「暫定 closure」へ下げること
- `sea_level_offset` を海面の正本へ寄せる方向
- basin / export を含む sediment accounting へ段階移行すること
- 観測量ベースの benchmark を整備すること

この proposal でまだ決めないこと:

- 最終的な state struct 名と永続化形式
- basin の離散化単位を `sink`、`depression hierarchy`、`drainage basin` のどれにするか
- sea-level equation をどこまで近似し、どこまで省略するか
- marine sediment の双方向交換をいつ導入するか

## 成功条件

- どの量を保存し、どの量が派生結果かを docs 上で明示できる。
- `Geology` / `Hydrology` / `Glaciology` / `Terrain` の責務境界で、solid / sediment / water / ice の混同が減る。
- `land fraction` の長期安定性を、reservoir diagnostics と観測量の両方で説明できる。
- 現行の零平均拘束を残す期間でも、その意味が「保存則」だと誤読されない。
- 後続実装で basin/export accounting と `sea_level_offset` 統一へ段階的に進められる。

## リスクとトレードオフ

- reservoir を分けると state と diagnostics が増え、実装コストも上がる。
- basin ごとの厳密収支は、全球一様スケールより計算負荷と複雑さが増す。
- sea level を inventory から決めると、既存の「海面=0 近似」に依存した処理の棚卸しが必要になる。
- 質量ベース uplift/subsidence は学術的には自然だが、完全な flexural / viscoelastic 解法まで行くと過剰に重い。

したがって、近似は許容する。
ただし何を近似し、何を保存しているかは明記する。

## 実施計画

1. docs 上で reservoir と派生量の語彙を確定する
2. `Geology` / `Hydrology` / `Glaciology` / `module_boundaries` に暫定用語を反映する
3. 全球一様 sediment budget を `global proxy` として診断出力に明記する
4. basin / sink 単位の sediment accounting を導入する
5. `marine_sediment_reservoir` と glacial sediment accounting を追加する
6. `sea_level_offset` を `ocean_water_inventory` と `ice_inventory` 由来へ寄せる
7. 零平均拘束を optional な numerical closure に格下げし、質量ベース項へ置換する
8. benchmark に hypsometry / relief / river flux / basin occupancy を追加する

## 決定事項

- v1 の `solid_earth_mass` はセル別正本にせず、`height`・密度 proxy・セル面積から導く全球 diagnostic proxy として持つ。
- v1 の `marine_sediment_mass` は一方向 sink とし、海成堆積物の双方向交換は将来方向として明記するが、初期実装には含めない。
- v1 の glacial sediment は `Hydrology` に渡さず、glacial erosion source の記録と export / marine accounting に留める。
- v1 の `ocean basin capacity` は現地形から毎 tick 近似再計算する。
- 観測比較の正本 artifact は `benches/results/` に置き、docs には判定基準と参照手順のみを置く。

## 参考

- Exner 型の sediment continuity
- glacial isostatic adjustment / load compensation literature
- sea-level equation と water inventory accounting の literature
- hypsometry と relief statistics を用いた地形比較の literature
