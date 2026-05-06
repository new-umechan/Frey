# changed field 名を `ChangedField` enum へ集約する

## Status

Accepted

## Context

`changed_fields` は外部互換のため文字列配列のまま維持したいが、
内部実装は文字列リテラル依存になっている。

## Decision

- 内部では `ChangedField` enum を使う
- 境界では `String` へ変換して既存フォーマットを維持する

## Consequences

利点:

- typo リスクが減る
- 追加 field の管理点が 1 箇所に集まる

コスト:

- enum の更新が必要になる
