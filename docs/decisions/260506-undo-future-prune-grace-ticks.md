# 260506 Undo Future Prune Grace Ticks

## Status

Accepted

## Context

単一 timeline では rewind 価値を優先して undo log を保持したい。
一方で、future log の扱いを固定にすると、運用ごとの最適値を選べない。

## Decision

- `TimelineRetentionPolicy` に `undo_future_prune_grace_ticks` を追加する
- undo prune では `current_tick + grace` を超える future log を優先 prune する
- grace 内の future log は past log と同じ距離基準で評価する
- `get_timeline_state` に grace 値を含める

## Consequences

- current-centric 方針を維持しつつ、future prune の強度を設定で調整できる
- retention policy の責務が増えるが、挙動が明示的になる
