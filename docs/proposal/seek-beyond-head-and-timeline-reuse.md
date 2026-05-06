# Seek Beyond Head And Timeline Reuse

## Status

Accepted

## 背景

単一 timeline モデルへ切り替えたことで、
`seek` は単なる checkpoint 復元ではなく timeline cursor の正本操作になった。

しかし実装上はまだ `seek(target_tick > head_tick)` を自然に扱えておらず、
`advance` も内部で `seek` と replay を使い分ける責務が散っている。

## 提案

- `seek_world_to_tick` は `target_tick <= head_tick` だけでなく
  `target_tick > head_tick` も受け付ける
- `target_tick > head_tick` の場合は、まず `head_tick` まで既存 timeline を再利用し、
  そこから不足分だけ新規 tick を計算する
- tick 実行ループを helper 化し、`advance` / `seek` / 将来の replay 系経路で共有する

## 成功条件

- `seek(head_tick + n)` が public API として成立する
- `rewind -> advance` と `seek(過去) -> seek(未来)` が head 再利用前提で一貫する
- 追加テストで `head_tick` 保持と metrics 等価性が固定される
