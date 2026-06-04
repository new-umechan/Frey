# サーバー事前計算 world 表示

## Status

Accepted

## Context

ブラウザ内 WASM で world simulation を進めると、初期生成と tick 進行の負荷がユーザー端末に乗る。表示側は既に `EngineClient` 抽象に依存しているため、計算をサーバー側に移し、ブラウザは同じ shape の field/metrics/delta を読む構成にできる。

## Decision

- HTTP API を提供する `precompute_server` と、bincode store を生成する `precompute_world` を Rust 側に追加する。
- v1 は CLI で固定 seed 一覧を事前計算し、ブラウザは `VITE_FREY_ENGINE=http` で API client に切り替える。
- 未計算 seed または mesh level は即時生成せず、生成リクエストとして queue に記録する。
- 再生は計算済み tick の範囲内だけ進める。
- 保存形式は JSON manifest + bincode frame とし、64 tick 間隔の keyframe と毎 tick delta を保存する。frame payload は後方互換を保ったまま `zstd` 圧縮を既定にする。

## Consequences

- ブラウザ側の simulation WASM は HTTP モードでは不要になる。
- keyframe + delta replay により seek 性能と保存容量のバランスを取れる。
- seed のオンデマンド生成 worker と queue 永続化は後続設計で詰める。

## Deferred Optimizations

- `auto play` と event jump は 1 tick 精度を必要とするため、260603時点では delta を毎 tick 保存する。
- delta 間引きは client または server の補完計算を必要とする。client 補完は simulation WASM を再導入し、server 補完は同時接続時の計算負荷と cache 設計を必要とするため、計算モデルが固まるまでは採用しない。
- Hashlife/Merkle chunk、表示専用 DTO、chunk 単位 delta などの容量削減は有望だが、store 形式と state 表現を複雑にする。現時点では早すぎる最適化になってしまうため保留する。
- 生成速度は開発速度に直結するため、容量削減より優先して改善する。まず release profile での生成コマンドと precompute 専用の短い retention policy を採用し、必要なら seed 単位の並列生成、module/phase 単位の並列化、keyframe/delta 書き込みの非同期化、生成済み tick の再利用を後続候補にする。
