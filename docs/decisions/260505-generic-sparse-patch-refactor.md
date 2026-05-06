# sparse patch コンテナを `SparsePatch<T>` に統一する

## Status

Accepted

## Context

現在の sparse patch 実装は型ごとに同じ struct と helper を繰り返している。
これは patch 種別の追加に対して保守コストが高い。

## Decision

- patch コンテナを `SparsePatch<T>` に統一する
- 既存の `SparseF32Patch` などの語彙は type alias として残す
- subsystem ごとの `UndoState` はそのまま維持する

## Consequences

利点:

- 型定義と helper の重複が減る
- 新しい patch 型の追加コストが下がる

コスト:

- generic helper の型境界を読む必要がある
