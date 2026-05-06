# Clock / Control を Undo State に揃える

## Status

Accepted

## Decision

- `clock` は `ClockUndoState` で保持する
- `control` は `ControlUndoState` で保持する
- `control.geology_params` が変わる場合のみ full fallback を許す

## Consequences

- small-state でも undo 表現の責務が一貫する
- `WorldCoreChangeSet` の生 clone 依存が減る
