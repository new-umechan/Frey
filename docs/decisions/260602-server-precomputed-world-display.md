# サーバー事前計算 world 表示

Status: Adopted

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
