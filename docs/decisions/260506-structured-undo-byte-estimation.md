# 260506 Structured Undo Byte Estimation

## Status

Accepted

## Context

単一 timeline の retention policy は `max_estimated_bytes` を使って prune 判断をする。
このため structured undo の保持量見積もりが粗いと、
undo window を過大評価または過小評価しやすい。

## Decision

- structured undo の byte 見積もり helper を追加する
- map patch は `(key, Option<value>)` を固定サイズで数えるのではなく、
  key bytes と value bytes を分離して加算する
- `entities` / `relations` の variable payload は個別 helper で見積もる

## Consequences

- retention の prune は依然として概算だが、structured undo の実量に近づく
- entity / relation payload が大きい timeline で budget 判断が安定しやすくなる
