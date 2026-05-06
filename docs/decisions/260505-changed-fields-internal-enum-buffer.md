# changed_fields の内部バッファを enum 化する

## Status

Accepted

## Context

`ChangedField` は導入済みだが、記録時点で文字列へ変換していた。
内部バッファを enum で持てば、型安全性と効率の両面で一貫する。

## Decision

- `finalize_tick_undo_log` 周辺の変更記録バッファは `Vec<ChangedField>` を使う
- 外部互換のため `TickUndoLog` には従来通り `Vec<String>` で保存する

## Consequences

利点:

- 文字列リテラル依存がさらに減る
- 変換境界が明確になる

コスト:

- 最終代入時に map 処理が必要
