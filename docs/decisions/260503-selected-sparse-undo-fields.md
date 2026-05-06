# 最初の sparse undo 対象は height と river 系に限定する

## Status

Accepted

## Context

subsystem 粒度の undo log は full snapshot より前進したが、
巨大な SoA 列が多い `geology` と `hydrology` では依然として粗い。

一方で、全列を一度に sparse 化すると実装範囲が大きすぎる。

## Decision

最初の sparse undo 対象を次の 3 列に限定する。

- `geology.height`
- `hydrology.river_flow`
- `hydrology.river_next`

selected field 以外に差分がない場合だけ sparse patch を使い、
それ以外は従来通り subsystem 全体コピーを保持する。

## Consequences

利点:

- 逆再生メモリ効率を段階的に改善できる
- 地形と河川の主要列から効果を得やすい
- 巻き戻しロジックを壊しにくい

コスト:

- geology / hydrology の undo state が full-or-sparse の分岐を持つ
- 最終的な汎用 diff にはまだ遠い

## Notes

この decision は first stage であり、後続で `erosion_rate` や climate 系へ拡張してよい。
