# Structured Undo Byte Estimation

## Status

Accepted

## 背景

`entities` / `relations` の undo は structured 化できているが、
retention 用の `estimated_bytes` は fixed-size 前提の概算がまだ残っている。

特に `PolityRecord.cells_cache`、`RegionRecord.cells`、`PolityGroup.members` のような
可変長 payload を多く持つ tick では、undo の実量より小さく見積もる。

## 提案

- structured undo の byte 見積もり helper を追加する
- map before-value patch は key bytes と value bytes を分けて見積もる
- `entities` / `relations` は variable payload を個別に加算する

## 成功条件

- `EntityUndoState` / `RelationsUndoState` の `estimated_bytes` が variable payload を反映する
- `application::world_` テストと build が通る
