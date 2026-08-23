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
- `GET /api/worlds/:world_id/stream` (WebSocket upgrade)
- `GET /api/worlds/:world_id/playback` (WebSocket upgrade)
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

`POST /api/worlds` は呼び出しごとに独立した session cursor を返す。同じ seed の immutable store は共有するが、
異なる browser の seek/advance は相互に干渉しない。

## Tick Stream

WebSocket 接続後、server は `catalog` を送る。client は次の `subscribe` を送り、中心 tick の近傍と粗い
keyframe を要求する。

```json
{
    "type": "subscribe",
    "request_id": 1,
    "center_tick": 57,
    "radius": 2,
    "known_exact_ticks": [55, 56, 57, 58, 59],
    "known_coarse_ticks": [0, 256, 512],
    "coarse_interval": 256,
    "include_coarse": true
}
```

server response は次の順序を取る。

- `catalog`: head tick、近傍半径上限、既定 coarse interval。
- `exact_anchor`: client が window 先頭を保持していない場合の全 field full frame、metrics、timeline。
- `exact_delta`: anchor または保持済み連続 frame の次から window 末尾までの保存済み delta。
- `coarse_frame`: `height`, `lake_depth`, `plate_id`, `river_flux`, `river_next`, `mantle_heat` の full frame。
- `complete`: request 単位の送信完了と実際の window 範囲。
- `error`: request の検証または store 読み込みエラー。接続自体は継続できる。

近傍半径は最大8、coarse interval は64--512に制限する。WebSocket は read-only であり、session cursor の確定は
従来の HTTP seek/advance が担う。store materialize は blocking worker で行い、server state lock は seed と head tick の
snapshot 取得時だけ保持する。

Web client は exact frame を8件、coarse frame を16件まで保持する。history slider の `input` は subscribe のみを送り、
`change` で seek を確定する。近傍 exact cache は即時同期し、遠方は coarse frame の主要 field を既存 full frame に重ねて
preview を表示した後、HTTP seek の exact state に差し替える。WebSocket が利用できない場合は従来の HTTP seek に fallback する。

## Playback Chunk Stream

`/playback` は連続再生だけに用いるbinary streamであり、`/stream` のseek previewとは独立している。clientは表示済みの
tickの次だけを要求し、zstd圧縮された保存済みdeltaをWorkerで展開して順に適用する。server session cursorはこのstreamでは
進めない。HTTP endpointを呼ぶ必要が生じた時だけ、clientが現在tickへseekして整合させる。

要求はJSON text frameで送る。

```json
{
    "type": "playback",
    "epoch": 12,
    "start_tick": 58,
    "tick_count": 1,
    "include_fields": ["height", "lake_depth", "river_flux", "river_next"]
}
```

応答はbinary `PlaybackChunk v1` frameである。先頭は `FRPB`、version (`u8`)、epoch (`u32 LE`)、tick (`u32 LE`)、
compressed payload length (`u32 LE`) の順で、残りはzstd payloadとなる。payloadは表示metadataとfield deltaのtyped-array値を
含む。`tick_count` は1--4に制限される。clientは先読みを最大8tickとし、seek/rewind時はepochを進めて古いframeを破棄する。

history sliderの入力は次の `preview` を使う。serverは指定tickをmaterializeし、指定fieldのfull deltaを同じbinary形式で返す。
clientは到着後に表示だけへ適用し、sliderの確定時にHTTP seekを行ってexact stateへ置換する。v1のpreviewは時間方向の
keyframeのみで、空間LODではない。

```json
{
    "type": "preview",
    "epoch": 13,
    "tick": 512,
    "include_fields": ["height", "lake_depth", "plate_id", "river_flux", "river_next", "mantle_heat"]
}
```

browserが `DecompressionStream("zstd")` を提供しない場合、clientはplayback streamを開かず既存のHTTP JSONへfallbackする。
このため対応していないbrowserでも表示の正しさは変わらないが、連続再生の通信量削減は得られない。

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

HTTP fallback と coarse preview 後の exact 差し替えでは、`get_field` request を height 取得後に並列実行する。
cell count が必要なため height が第1段、それ以外が第2段となり、field 数に比例する RTT 累積は発生しない。
