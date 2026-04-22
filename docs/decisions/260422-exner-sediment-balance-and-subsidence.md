# Exner系 sediment 収支制約と明示的沈降への移行

## Status

Accepted

## Context

- `260421-land-freeboard-control` では、全陸化ドリフトを全球 freeboard 一様シフトで抑えていた。
- これは局所勾配を壊しにくい一方、侵食・堆積の質量収支を直接は改善しない。
- 現行実装では `Hydrology` が算出する `deposition_rate` が `erosion_rate` 総量を上回る tick があり得る。
- 既定パラメータの沈降項が 0 で、構造隆起・火山隆起・堆積に対する長期的な下向き復元力が不足していた。

## Decision

- `Geology` への hydrology 反映では、tick ごとの fluvial `deposition_rate` 総量を
  `erosion_rate` 総量と mobile sediment inventory の範囲内に制限する。
- 制限はセルごとの相対分布を保つ一様スケールで行う。
- `Σerosion > Σdeposition` の差分は、未解像の海盆・深海への sediment export とみなす。
- glacial erosion は現段階では transport を持たないため、export 扱いにする。
- `tectonic_subsidence_gain` と `thermal_subsidence_gain` を小さな正値へ戻す。
- `Geology` の内生的な鉛直変位は、1 tick ごとの全球平均変位が 0 になるように拘束する。
- runtime では海陸比を合わせるための全球 terrain shift を行わない。
- 動的海面は `sea_level_offset` を正本とし、必要なら各モジュールをそちらへ段階的に寄せる。
- `target_sea_ratio` への復元は、地形全体ではなく `sea_level_offset` の弱い緩和で表現する。

## Rationale

- Exner 系の基本は sediment mass conservation であり、少なくとも fluvial 成分の総量非発散は守るべきである。
- 沈降は uplift と対になる一次の下向き項であり、freeboard の後段補正より因果が明確である。
- tectonics / diffusion / isostasy のような内生過程だけで全球平均標高が単調に動くのは、閉じた固体在庫系として不自然である。
- 一様スケールは basin ごとの厳密収支より粗いが、既存構造への変更量が小さく、挙動も解釈しやすい。
- 全球 terrain shift は原因系ではなく観測量を直接動かすため、`sea_level_offset` と併用すると
  海面の意味が二重化して解釈しづらい。
- そのため、補正は sediment budget と subsidence に限定し、海面は `sea_level_offset` 側へ集約する。

## Consequences

- 地形更新は「堆積過多を最後に押し戻す」方式から、「堆積原資を超えた aggradation を適用しない」方式へ変わる。
- `deposition_rate` の公開値は、適用後の制約済み値と一致する。
- hydrology 反映だけでは全球平均標高を動かさないため、`sea_level_offset` を使う系との衝突を避けられる。
- 海陸比の復元は海面側で起こるため、起伏そのものは保持されやすい。
- glacial sediment や basin 単位の厳密収支は今後の課題として残る。
- `260421-land-freeboard-control` は superseded とする。
