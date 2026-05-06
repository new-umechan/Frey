# changed field 名カタログを query から公開する

## Status

Accepted

## Context

changed field 名の typo リスクを下げるため enum 化したが、
外部側で利用可能なカタログがないと同じ文字列集合を重複保持しやすい。

## Decision

- `ALL_CHANGED_FIELDS` を公開する
- query/wasm から一覧を返す API を追加する

## Consequences

利点:

- transport 側が runtime 正本に追従できる
- field 名検証の基盤になる

コスト:

- API surface が 1 つ増える
