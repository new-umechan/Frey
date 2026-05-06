# finalize の small-struct 差分検出を helper 化する

## Status

Accepted

## Context

`finalize_tick_undo_log` は `UndoState::from_diff` 導入で薄くなったが、
最後に残る small-struct 差分判定は still repetitive である。

## Decision

- `record_change_if_different` helper を導入する
- `core_change_set` の scalar/small-struct 更新に適用する

## Consequences

利点:

- 終盤の実装が短くなる
- フィールド追加時の変更パターンが揃う

コスト:

- helper を追うために 1 段参照が増える
