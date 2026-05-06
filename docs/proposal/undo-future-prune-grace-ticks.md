# Undo Future Prune Grace Ticks

## Status

Accepted

## 背景

undo log の current-centric prune は導入済みだが、
future 側を常に最優先で落とす固定挙動だと、
ユースケースによっては未来近傍の log を残したい場面がある。

## 提案

- retention policy に `undo_future_prune_grace_ticks` を追加する
- `current_tick + grace` 以内の future log は「即 prune 対象」にしない
- `undo_log_limit` 超過と budget 超過の両経路で同じ閾値を適用する
- `get_timeline_state` から現在の grace 値を観測可能にする

## 成功条件

- graceful に future 優先 prune 強度を調整できる
- 既定値では従来の current-centric 方針を維持する
- `application::world_` テストと build が通る
