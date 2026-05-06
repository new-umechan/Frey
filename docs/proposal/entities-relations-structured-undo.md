# Entities Relations Structured Undo

## Status

Accepted

## 背景

`entities` と `relations` は `WorldCoreChangeSet` の中で
まだ full clone のまま残っている。

`entities` は疎な record 集合なので、
SoA の sparse patch より create/update/delete ベースの undo が向いている。
`relations` も map の before-values patch で扱える。

## 提案

- `EntityUndoState` を追加する
    - create 後に消す id
    - before record に戻す upsert
- `RelationsUndoState` を追加する
    - relation map の before-values patch
    - `polity_groups` は変更時に full before value

## 成功条件

- `WorldCoreChangeSet.entities` / `relations` が full clone から外れる
- `application::world_` テストと build が通る
