# hydrology sink 系列と ecology 公開 state を sparse undo 対象へ広げる

## Status

Accepted

## Context

現在の sparse undo は `hydrology` の river 系と一部連続値列、
および `geology` / `climate` / `glaciology` の主要列まで広がっている。
一方で `hydrology` の sink 系列と `ecology` はまだ subsystem 全体コピーが主体である。

## Decision

next stage の sparse undo 対象として次を追加する。

- `hydrology` の sink 系 selected field
- `ecology` の `biome` と公開連続値列

実装方針は以下とする。

- `bool` / `u8` / `u32` 向けの sparse patch 型を追加する
- `hydrology.river_downstream` と `ecology_internal` は full copy fallback 条件として残す
- selected field だけが変化した tick では field 単位の before-values を保存する

## Consequences

利点:

- `hydrology` の full copy 発生率をさらに下げられる
- `ecology` を SoA に沿った undo 表現へ移せる

コスト:

- patch 型と適用 helper の数は増える
- mixed-type field の扱いを整理しないと可読性が落ちやすい
