# Polity Groups Structured Undo

## Status

Accepted

## 背景

`RelationsUndoState` は relation map を before-value patch に移せているが、
`polity_groups` だけは変更時に `Vec<PolityGroup>` 全体を保持している。

`polity_groups` は group 数が多くなるほど retention 効率を悪化させる。
一方で `Vec` なので、単純な upsert/remove だけでは before 側の順序を失う。

## 提案

- `PolityGroupsUndoState` を追加する
    - changed / removed group の before payload を `upserts` として保持する
    - after 側で新規作成された group id は `removals` に保持する
    - before 側の group order は `order_before` として別保持する
- undo 適用時は `removals` と `upserts` を反映したあと、
  `order_before` に従って `Vec<PolityGroup>` を再構成する

## 成功条件

- `RelationsUndoState.polity_groups` が full snapshot ではなく structured undo になる
- `application::world_` テストと build が通る
