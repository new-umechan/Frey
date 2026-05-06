# TimelineRuntime の導入

## Status

Accepted

## Context

現行実装では `ManagedWorld` と `TimelineArchive` が `WorldService` で別 map 管理されている。
この構成は現状機能には足りるが、時間軸責務の正本が不明確である。

今後、逆再生向けに `TickUndoLog`、cursor、branch を導入するなら、
checkpoint / intervention / undo log を束ねる runtime 構造が必要になる。

## Decision

- `TimelineRuntime` を application runtime の正式型として追加する
- `TimelineRuntime` は `TimelineArchive` と `TickUndoLog` 群を持つ
- `WorldService` は archive 単体ではなく `TimelineRuntime` を保持する
- `TickUndoLog` は今回 placeholder 定義だけ導入し、実データ記録は後続で行う

## Consequences

利点:

- 時間軸責務の置き場所が固定される
- seek / fork / rewind 実装の追加先が明確になる
- `WorldService` の API が時間軸中心に整理される

コスト:

- 既存 use case の引数名や取得経路の更新が必要になる
- 初期段階では `TimelineRuntime` が薄く見える

## Notes

今回の decision は scaffold であり、逆再生機能そのものの完成を意味しない。
