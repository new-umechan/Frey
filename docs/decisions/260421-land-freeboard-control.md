# Land Freeboard 制御の導入

## Status

Superseded

Replaced by: `docs/decisions/260422-exner-sediment-balance-and-subsidence.md`

## Context

- 地形進化の長期実行で陸面積が単調増加し、`tick=250` で全陸化するケースが確認された。
- 既定パラメータで沈降項が無効（`tectonic_subsidence_gain=0`, `thermal_subsidence_gain=0`）であり、将来的な較正対象である。
- 実装上は海面判定が `0.0` と `sea_level_offset` で混在しており、全面置換は大改修になる。

## Decision

- 当時は `Geology` の最終反映後に、`target_sea_ratio` を使った quantile ベースの freeboard 漸近補正を導入した。
- その後、全球一様シフトは海陸比制御としては有効でも、侵食・堆積の質量収支を直接改善しないため、
  Exner系の収支制約と明示的沈降へ置き換える判断に改めた。

## Rationale

- 一様シフトは局所勾配を変えないため、短期の安全策としては妥当だった。
- ただし、海陸比ドリフトの原因を sediment budget と subsidence で扱わず、
  後段の全球オフセットで打ち消す構成は長期運用の正本に向かなかった。

## Consequences

- freeboard 補正は短期の暫定策として位置づけ直す。
- 正本は `docs/decisions/260422-exner-sediment-balance-and-subsidence.md` の方針へ移る。
