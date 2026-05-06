# Changed Fields Enum

## Status

Accepted

## 背景

`TickUndoLog.changed_fields` は `Vec<String>` で保持されるが、
記録時は文字列リテラルを都度 `push` している。
この方式は typo をコンパイル時に検出できない。

## 目的

- changed field 名を型安全に扱う
- `finalize_tick_undo_log` 周辺の記述を統一する

## 提案

- `ChangedField` enum を導入する
- `as_str()` と `push_changed_field()` helper で `Vec<String>` へ記録する
- 既存の `Vec<String>` 形式は維持する

## 成功条件

- 既存 API 互換は維持される
- `application::world_` テストが通る
