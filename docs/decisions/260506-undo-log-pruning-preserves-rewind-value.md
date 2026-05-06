# 260506 Undo Log Pruning Preserves Rewind Value

## Status

Accepted

## Context

単一 timeline で逆再生体験を支えるのは、
`current_tick` 近傍の undo log である。

oldest-first prune は実装が簡単だが、
予算超過時に直近の巻き戻し足場まで失いやすい。

## Decision

- undo log prune は rewind 価値ベースにする
- `current_tick` の undo log は代替がある限り保護する
- prune 候補は future 側、その次に距離の遠い tick を優先する
- この基準を `undo_log_limit` 超過と retention budget 超過の両方に適用する

## Consequences

- 直近巻き戻しの体験を維持しやすくなる
- oldest-first より複雑になるが、単一 timeline の操作意図に沿う
- seek 価値ベース checkpoint prune と対になる方針になる
