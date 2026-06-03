# Precomputed Server API

`precompute_server` はサーバー側で seed ごとの world を事前計算し、ブラウザに `EngineClient` 互換の JSON API を提供する。

## Runtime

- `precompute_world` CLI が seed ごとの bincode store を生成する。
- `precompute_server` は `FREY_PRECOMPUTE_STORE_DIR` 配下の manifest を読み、HTTP API を提供する。
- 未計算 seed または異なる mesh level は生成リクエストとして `202 Accepted` を返す。

## Store

- manifest は JSON、frame は bincode payload を `none` または `zstd` で保存する。
- keyframe は表示用 full frame を保存する。
- delta は `ViewDeltaResponse` 互換 payload を保存する。
- 既定は `tick=1600`、keyframe は 64 tick 間隔、delta は毎 tick。
- 新規生成の既定圧縮は `zstd`、manifest に圧縮方式がない既存 store は `none` として読む。

## Endpoints

- `GET /api/health`
- `GET /api/precomputed/seeds`
- `POST /api/precompute-requests`
- `GET /api/mesh/:level`
- `POST /api/worlds`
- `POST /api/worlds/:world_id/advance`
- `POST /api/worlds/:world_id/advance-slice-and-delta`
- `POST /api/worlds/:world_id/view-delta`
- `GET /api/worlds/:world_id/metrics`
- `GET /api/worlds/:world_id/timeline`
- `GET /api/worlds/:world_id/field/:field_kind?lod=1`
- `GET /api/worlds/:world_id/checkpoints`
- `POST /api/worlds/:world_id/seek`
- `POST /api/worlds/:world_id/rewind`
- `POST /api/worlds/:world_id/simulation-rate`
- `GET /api/exec-modules`
- `GET /api/exec-module-graph`

## Delta Semantics

`advance-slice-and-delta` は計算済み tick の範囲でのみ進む。最終 tick では `processed_ticks: 0` と `delta: null` を返す。

`advance-slice-and-delta` は保存済み delta を返す。`include_fields` が指定された場合は指定 field のみ返す。

`view-delta` は現在 session tick の materialized frame から full delta を返す。seek 後の表示同期に使う。
