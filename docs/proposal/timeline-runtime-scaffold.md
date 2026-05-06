# Timeline Runtime Scaffold

## Status

Superseded

## Replaced by

`../decisions/260503-timeline-runtime-scaffold.md`

## 背景

用語は `timeline` / `checkpoint` / `seek` / `view delta` に整理したが、
実装はまだ `ManagedWorld` と `TimelineArchive` を別管理している。

この構成では、将来 `TickUndoLog` や timeline cursor を導入するときに、
時間軸責務が `WorldService` と use case に散らばる。

## 目的

- 時間軸責務の入れ物として `TimelineRuntime` を先に導入する
- 将来の `TickUndoLog` と cursor を置く場所を固定する
- 現行挙動を維持したまま、`archive` 単体依存を減らす

## 提案概要

- `TimelineArchive` は checkpoint / intervention の保存責務に限定する
- `TimelineRuntime` を追加し、`archive` と `undo_logs` を束ねる
- `WorldService` は `archive` ではなく `timeline runtime` を保持する
- `TickUndoLog` は今回 placeholder として導入し、実データ記録は次段階で実装する

## スコープ

- `rust/src/application/world_runtime.rs`
- `rust/src/application/world_service.rs`
- `rust/src/application/world_use_cases.rs`
- 関連 reference docs

今回は次をスコープ外とする。

- `TickUndoLog` の内容実装
- timeline cursor / branch head の導入
- rewind API の追加

## 成功条件

- `WorldService` が timeline 用の runtime 構造を保持する
- checkpoint / intervention の参照が `TimelineRuntime` 経由に寄る
- 既存の seek / fork / checkpoint 列挙テストが維持される

## リスクとトレードオフ

- 途中段階では `TimelineRuntime` が薄い wrapper に見える
- `undo_logs` は placeholder のため、現時点では機能より構造整理の意味が強い

ただし、この scaffold がないまま `TickUndoLog` を実装すると、
後から責務再配置の大きな差し替えが必要になる。

## 実施計画

1. `TimelineRuntime` と `TickUndoLog` を追加する
2. `WorldService` を archive map から timeline map へ切り替える
3. use case と query を timeline 経由へ更新する
4. docs とテストを追従させる

## 未解決事項

- `TimelineRuntime` が将来 `TimelineCursorState` を内包するか
- `undo_logs` の保存窓を checkpoint と別管理にするか
