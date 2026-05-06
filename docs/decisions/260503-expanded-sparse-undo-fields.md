# sparse undo 対象を climate と erosion/deposition へ広げる

## Status

Accepted

## Context

first stage では `height` と `river_flow` / `river_next` だけを sparse 化した。
しかし、tick ごとの主要更新では `temperature` / `precipitation` や
`erosion_rate` / `deposition_rate` も頻繁に変化する。

## Decision

second stage の sparse undo 対象として次を追加する。

- `climate.temperature`
- `climate.precipitation`
- `hydrology.erosion_rate`
- `hydrology.deposition_rate`

selected field 以外も変わった tick では、従来通り full subsystem copy にフォールバックする。

## Consequences

利点:

- climate / hydrology の full copy へ落ちる tick が減る
- undo log のメモリ効率が段階的に改善する

コスト:

- climate / hydrology の undo state がさらに full-or-sparse 混在になる
- テストと比較ロジックは少し複雑になる

## Notes

この decision は second stage の拡張であり、最終的な汎用 sparse diff ではない。
