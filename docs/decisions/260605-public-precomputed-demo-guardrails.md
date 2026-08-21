# 公開 precomputed demo の制限をサーバ側で強制する

## Status

Accepted

## Context

`precompute_server` はブラウザに `EngineClient` 互換 API を提供し、保存済み
keyframe/delta を返す。公開デモでは、任意 seed や任意 mesh level を受けると
負荷予測が難しくなり、未計算 seed の queue が外部入力で増える。

Web 側に公開 seed 一覧を持たせるだけでは、直接 HTTP request を送れば制限を
迂回できる。そのため、公開デモの制限は server side policy として扱う必要がある。

## Decision

公開デモ向けの制限を env で設定できるようにする。

- `FREY_PUBLIC_SEEDS` に含まれない seed は拒否する。
- `FREY_PUBLIC_MESH_LEVEL` と一致しない mesh level は拒否する。
- `FREY_MAX_MESH_LEVEL` を超える mesh generation は拒否する。
- `FREY_MAX_TICK` を超える seek/head tick は公開しない。
- `FREY_MAX_LOD` を超える field LOD は拒否する。
- `FREY_DISABLE_PRECOMPUTE_REQUESTS=true` の場合、未計算 seed の queue 作成を拒否する。
- `FREY_CORS_ORIGINS` が指定された場合、CORS origin をその一覧に限定する。

公開 UI は Cloudflare Pages、API は外部 host 上の `precompute_server` を
Cloudflare Tunnel 経由で公開する。world の事前計算と store の保持は外部 host で行い、
Cloudflare Worker/R2 への移植はこの公開方式の範囲に含めない。

既存の開発挙動を保つため、これらの env が未指定の場合は従来通り permissive CORS と
未計算 seed の queue 記録を許可する。

## Consequences

- 公開デモでは server side allowlist により、Web UI を迂回した任意 seed 利用を防げる。
- `mameta` のような手元 machine を Cloudflare Tunnel 経由で公開する場合も、API の
  abuse surface を小さくできる。
- rate limit、認証、observability は proxy/hosting 側の責務として残る。

現在の API 制限と運用手順は
`docs/reference/interface/precomputed_server_api.md` および
`docs/operations/precomputed_server.md` を正本とする。
