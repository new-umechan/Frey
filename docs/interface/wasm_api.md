# WASM API仕様（初版）

本書は、JSから利用するWASM公開APIの仕様を定義する。
実装済みAPIと未実装APIを同じ文書で管理し、差分を追跡できる形にする。

## 1. 時間管理API

### 1.1 `tick() -> f64`（実装済み）

`tick` は世界の累積管理Tickカウンタを返す。
返り値はRust内部のu64カウンタ値をf64へ変換した値である。

注意:
- 時間単位（年、万年など）を直接返すAPIではない
- 時代名は返さない
- カウンタ値として単調増加する

### 1.2 `step(ticks: u32) -> void`（実装済み）

管理Tickを進める。
`ticks` の値だけ進む。
`ticks=0` の場合は進まない。

### 1.3 `eraKey() -> string`（実装済み）

現在時代のキーを返す。
返却値は `crust` / `environment` / `life` / `civilization` / `history` のいずれか。

## 2. Checkpoint API（未実装）

初版はメモリ内チェックポイントのみを対象にする。
永続化（JSON export/import）と差分保存は対象外。

### 2.1 型

- `CheckpointId`: string
- `CheckpointSummary`:
  - `id: CheckpointId`
  - `tick: f64`
  - `era: string`

### 2.2 API

- `save_checkpoint() -> CheckpointId`
- `load_checkpoint(id: CheckpointId) -> void`
- `list_checkpoints() -> CheckpointSummary[]`

### 2.3 エラー方針

- `load_checkpoint` で存在しないidを指定した場合は例外
- チェックポイントが1件もない場合の `list_checkpoints` は空配列

## 3. レイヤー取得API（未実装）

### 3.1 `get_layer(kind: string) -> Float32Array`

初版は常にFloat32Arrayを返す。
未生成レイヤーと不正kindは例外とする。
nullは返さない。

kindの例:
- `climate.temp`
- `climate.rain`
- `ecology.habitability`
- `ecology.productivity`
- `civilization.population`

将来拡張:
- 返却形式を `{ data, meta }` へ拡張する可能性がある
- 初版は呼び出し側の単純性を優先して配列直返しに固定する

## 4. ステータス

- 実装済み:
  - `tick`
  - `step`
  - `eraKey`
- 未実装:
  - `save_checkpoint`
  - `load_checkpoint`
  - `list_checkpoints`
  - `get_layer`
