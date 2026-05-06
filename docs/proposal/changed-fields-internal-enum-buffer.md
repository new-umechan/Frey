# Changed Fields Internal Enum Buffer

## Status

Accepted

## 背景

`ChangedField` enum を導入後も、記録バッファは `Vec<String>` のまま更新していた。
この構造だと途中段階で文字列生成が発生する。

## 目的

- 内部処理を型安全な enum バッファで統一する
- 文字列変換を境界で 1 回に限定する

## 提案

- `finalize_tick_undo_log` の内部では `Vec<ChangedField>` を使う
- `TickUndoLog.changed_fields` へ代入する直前に `Vec<String>` へ変換する

## 成功条件

- 外部フォーマット互換は維持される
- `application::world_` テストが通る
