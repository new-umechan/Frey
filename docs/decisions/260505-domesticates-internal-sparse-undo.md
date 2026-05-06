# Domesticates Internal を Sparse Undo 化する

## Status

Accepted

## Decision

- `DomesticatesUndoState` は `domesticates_internal` を `SparsePatch<DomesticatesInternal>` で保持する
- `domesticates` subsystem の full fallback は、将来さらに複合構造が増えた場合に限る

## Consequences

- `domesticates` の full clone 発生率が下がる
- retention 効率がさらに改善する
