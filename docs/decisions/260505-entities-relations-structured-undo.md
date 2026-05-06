# Entities / Relations を Structured Undo 化する

## Status

Accepted

## Decision

- `entities` は record 単位の upsert/remove undo で保持する
- `relations` は map の before-values patch で保持する
- `polity_groups` は vector 全体の before value を保持する

## Consequences

- raw clone 依存がさらに減る
- `entities` の巻き戻しがドメイン構造に沿ったものになる
