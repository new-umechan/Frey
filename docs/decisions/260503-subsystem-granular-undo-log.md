# TickUndoLog は subsystem 粒度で圧縮する

## Status

Accepted

## Context

`TickUndoLog` は導入済みだが、現状は tick 開始時 snapshot をそのまま保持している。
この方式は正確だが、undo log の意図とズレている。

一方で、直ちにセル列単位の sparse diff へ移行すると実装範囲が広い。

## Decision

- `TickUndoLog` は tick 完了後に `WorldCoreChangeSet` へ圧縮する
- change set はまず subsystem 単位の before 値を保持する
- 巻き戻しは change set の before 値を書き戻すことで実現する
- 将来の細粒度 diff 化はこの `WorldCoreChangeSet` を分解して進める

## Consequences

利点:

- snapshot ベースから before-values ベースへ一歩進められる
- 既存挙動を壊しにくい
- 後続の細粒度化の境界が明確になる

コスト:

- subsystem 全体が少しでも変わると、その subsystem 全体を保存する
- 最終形よりメモリ効率はまだ粗い

## Notes

この decision は最終形ではなく、中間段階の構造固定である。
