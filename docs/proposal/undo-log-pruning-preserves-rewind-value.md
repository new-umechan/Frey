# Undo Log Pruning Preserves Rewind Value

## Status

Superseded by `../decisions/260506-undo-log-pruning-preserves-rewind-value.md`

## 背景

checkpoint prune は seek 価値を基準に改善されたが、
undo log prune はまだ oldest-first が残っている。

この方式だと、`current_tick` 近傍の巻き戻し足場が
予算超過時に先に削れる可能性がある。

## 提案

- undo log prune は `current_tick` からの rewind 価値を基準に行う
- `current_tick` の undo log は代替候補がある限り優先保持する
- prune 候補は次を優先する
    - `current_tick` より未来側の undo log
    - `current_tick` から遠い undo log
- `undo_log_limit` 超過時と `max_estimated_bytes` 超過時の両方で同じ基準を使う

## 成功条件

- `undo_log_limit` 超過時に oldest 一辺倒で減らさない
- `current_tick` 近傍の rewind 足場が残りやすくなる
- `application::world_` テストと build が通る
