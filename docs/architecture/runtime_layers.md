# Runtime Layers

## 目的

この文書は、実行時レイヤ構造を定義する。
目的は次の 3 つである。

- simulation core を transport と UI 都合から切り離す
- Web / WASM / CLI などの入口を差し替え可能にする
- snapshot / replay / intervention を application 層の責務として固定する

## レイヤ構成

正本は、次の 4 層とする。

### 1. `core`

世界状態の正本と module 実行を持つ層。

- `World`
- `WorldState`
- `WorldProjectionState`
- `EntityState`
- `ClockState`
- `ModuleDeclaration`
- tick 実行
- seed 再現性
- snapshot の計算に必要な最小データ

この層は UI、WASM、Worker、ブラウザ API に依存しない。

### 2. `application`

core を使ってユースケースを提供する層。

- world 初期化
- work budget 付き slice 実行
- snapshot / replay
- intervention 適用
- debug / metrics / field query

この層は transport 非依存の API を公開し、DTO を返す。

application の責務は、次の 3 種に分ける。

- `WorldService`
  world registry / archive registry / world id 発番を担当する
- `WorldUseCases`
  `init_world`、`exec_world`、`exec_world_slice`、`restore_world_to_tick`、`fork_world` などの業務フローを担当する
- `WorldRuntime`
  `ManagedWorld`、`WorldArchive`、`WorldTransportCache` など、world 操作に付随する実行時状態を担当する

transport 層の controller は、application の use case を呼び出し、
serialization とエラー境界の変換だけを行う。

### 3. `transport`

application を外部境界へ接続する層。

- WASM binding
- Worker message protocol
- 将来の CLI / server adapter

transport は serialization、message 境界、エラー変換だけを担当する。
世界更新の業務ロジックを持ち込まない。

### 4. `presentation`

描画と操作を担当する層。

- Three.js scene
- DOM / HUD
- playback controller
- debug overlay

presentation は world 正本を持たず、transport 経由の view model を消費する。

## 依存方向

依存は常に内側へ向かう。

`presentation -> transport -> application -> core`

逆方向依存は置かない。
特に `core -> wasm_bindgen`、`core -> browser API`、`core -> worker protocol` は禁止する。

## 実装方針

全面移行は一度に行わない。まず次の順で進める。

1. Rust の純粋計算 API を `core_api` として分離する
2. WASM 公開関数を `transport::wasm` に寄せる
3. Web の generated wasm 直参照を `web/src/transport/wasm/` に集約する
4. `WorldSimController` のユースケースを application 層へ切り出す
5. `interface/` を段階的に削除し、presentation は transport facade のみを見る

## 実装規約

Web 側の WASM 入口は `web/src/transport/wasm/` を正本とし、
`interface/wasm.ts` 互換 shim は置かない。

Rust 側でも `lib.rs` は既存の公開関数を維持するが、実体は `core_api` と `transport::wasm` へ委譲する。

## 補足

`application` 層を明示すると、今後追加される以下の機能を整理しやすい。

- 介入ログ保存
- 分岐世界の fork
- replay / restore
- 検証用の deterministic run
- headless benchmark 実行
