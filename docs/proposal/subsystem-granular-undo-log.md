# Subsystem Granular Undo Log

## Status

Accepted

## 背景

現状の `TickUndoLog` は checkpoint snapshot をそのまま保持しており、
巻き戻しはできるが、実質的には tick 単位 snapshot の保存に近い。

これは scaffold としては十分だが、逆再生前提の再設計としては
`before_values` ベースの undo log へ段階的に寄せる必要がある。

## 目的

- full snapshot ベースの undo log から前進する
- 実装コストを抑えつつ、`before_values` ベースへ移行する
- 将来のセル列単位 diff へ繋がる構造を導入する

## 提案概要

- `CheckpointSnapshot` を長期保持する代わりに、tick 完了後に `WorldCoreChangeSet` へ圧縮する
- change set はまず `WorldState` の各 subsystem、`entities`、`relations`、`clock`、`control` を単位に持つ
- 巻き戻しは current world に対して change set の `before_values` を上書きする
- `hydrology_dynamics` / `geology_dynamics` / `applied_intervention_seq` も before 値として保持する

## スコープ

- application runtime の `TickUndoLog`
- `rewind_world_by_ticks`
- application テスト

今回は次をスコープ外とする。

- セル index 単位の sparse diff
- entity patch 単位の差分化
- view delta と undo log の統合表現

## 成功条件

- `TickUndoLog` が tick 完了後に full snapshot ではなく subsystem change set を持つ
- `rewind_world_by_ticks` がその change set から状態復元できる
- 既存の rewind 等価性テストが通る

## リスクとトレードオフ

- subsystem 全体コピーなので、最終形よりはまだ粗い
- ただし full snapshot よりは時間軸用の責務が明確になり、後続の細粒度 diff に繋げやすい

## 実施計画

1. `WorldCoreChangeSet` を追加する
2. tick 開始時は一時 snapshot を持つ
3. tick 完了後に snapshot と current world を比較して change set へ圧縮する
4. rewind は change set を current world に適用する

## 未解決事項

- どの subsystem から cell 列単位 sparse diff に落とすか
- glaciology / hydrology 系の巨大列をどの順で細粒度化するか
