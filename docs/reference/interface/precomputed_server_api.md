# Precomputed Server API

`precompute_server` はサーバー側で seed ごとの world を事前計算し、ブラウザに `EngineClient` 互換の JSON API を提供する。

## Runtime

- `precompute_world` CLI が seed ごとの bincode store を生成する。
- `precompute_server` は `FREY_PRECOMPUTE_STORE_DIR` 配下の manifest を読み、HTTP API を提供する。
- 未計算 seed または異なる mesh level は生成リクエストとして `202 Accepted` を返す。
- 公開デモ制限 env が指定されている場合、allowlist 外 seed、mesh level、tick、LOD は `403 Forbidden` を返す。
- `FREY_DISABLE_PRECOMPUTE_REQUESTS=true` の場合、未計算 seed の生成リクエストは queue に入れず `403 Forbidden` を返す。

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

## Public Demo Limits

`precompute_server` は公開デモ用に次の env を読む。

- `FREY_PUBLIC_SEEDS`: comma-separated seed allowlist。
- `FREY_PUBLIC_MESH_LEVEL`: 公開 mesh level。指定時は一致しない request を拒否する。
- `FREY_MAX_MESH_LEVEL`: mesh generation の上限。
- `FREY_MAX_TICK`: 公開 head tick と seek 上限。
- `FREY_MAX_LOD`: field endpoint の LOD 上限。
- `FREY_DISABLE_PRECOMPUTE_REQUESTS`: `true` の場合、生成 request queue を無効化する。
- `FREY_CORS_ORIGINS`: comma-separated CORS origin allowlist。未指定時は開発互換の permissive CORS。

Web 側は `VITE_FREY_DEMO_SEEDS_URL` が指定された場合、その JSON を seed catalog として読み、
任意 seed 入力の代わりに公開 seed selector を表示する。JSON は次の shape を持つ。

```json
{
    "seeds": [
        {
            "seed": "alpha",
            "label": "alpha",
            "mesh_level": 6,
            "description": "公開デモの標準 world"
        }
    ]
}
```

## Delta Semantics

`advance-slice-and-delta` は計算済み tick の範囲でのみ進む。最終 tick では `processed_ticks: 0` と `delta: null` を返す。

`advance-slice-and-delta` は保存済み delta を返す。`include_fields` が指定された場合は指定 field のみ返す。

`view-delta` は現在 session tick の materialized frame から full delta を返す。seek 後の表示同期に使う。
