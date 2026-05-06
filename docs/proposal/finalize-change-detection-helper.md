# Finalize Change Detection Helper

## Status

Accepted

## 背景

`finalize_tick_undo_log` の後半には、`domesticates` 以降の
small-struct / scalar 系差分判定が直列で並んでいる。
`if before != after { changed_fields.push(...); set Some(before); }`
のパターンが重複している。

## 目的

- `finalize_tick_undo_log` の終盤を薄くする
- 差分検出パターンの重複を減らす

## 提案

- generic な `record_change_if_different` helper を追加する
- `domesticates` 以降の差分検出に適用する

## 成功条件

- 挙動は不変
- `application::world_` テストが通る
