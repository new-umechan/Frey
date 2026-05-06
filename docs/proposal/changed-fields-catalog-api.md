# Changed Fields Catalog API

## Status

Accepted

## 背景

`ChangedField` は runtime 内部の正本化が進んだが、
transport/query から有効な changed field 名を取得する経路がない。

## 目的

- changed field 名の正本を query 側でも再利用する
- クライアント側の検証や補完に使えるカタログ API を提供する

## 提案

- `world_runtime` で `ALL_CHANGED_FIELDS` を公開する
- `world_query_use_cases` に `list_changed_fields` を追加する
- WASM API に `list_changed_fields_js` を追加する

## 成功条件

- 文字列正本が runtime に一元化される
- `application::world_` テストが通る
