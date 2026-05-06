# Retention は Timeline Anchor を優先保持する

## Status

Accepted

## Context

単一 timeline では future を破棄しないため、
checkpoint pruning の粗さがそのまま seek 可能性の喪失につながる。

## Decision

- retention pruning は古い undo log を先に捨てる
- checkpoint は初期 anchor と最新 anchor を優先保持する
- anchor しか残らない場合、budget 超過でもそれ以上は prune しない

## Consequences

利点:

- 最低限の replay 足場を維持できる
- `seek` と `head_tick` 再利用の失敗率を下げられる

コスト:

- 厳密な budget 遵守より timeline 可用性を優先するため、
  `total_estimated_bytes <= max_estimated_bytes` を常に保証しない
