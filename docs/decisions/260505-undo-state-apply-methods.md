# undo 適用責務を `UndoState::apply_to` へ寄せる

## Status

Accepted

## Context

patch コンテナは汎用化されたが、適用処理は依然として use case 層で
多数の field を直接列挙している。

## Decision

- subsystem ごとの sparse/full 適用ロジックは `UndoState` に持たせる
- use case 層は `WorldCoreChangeSet` の orchestration に専念する

## Consequences

利点:

- subsystem 固有ロジックが runtime 側にまとまる
- `apply_core_change_set` の可読性が上がる

コスト:

- `world_runtime.rs` の責務は少し増える
