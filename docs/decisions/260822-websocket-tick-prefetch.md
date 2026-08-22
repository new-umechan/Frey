# WebSocket tick streaming と二段階 prefetch cache

## Status

Accepted

## Context

公開 precompute server の history seek は、`POST /seek` の完了後に約 47 field を HTTP で直列取得する。
このため store の materialize が速くても、remote API では request latency が field 数だけ累積する。

level 6 の実測では全 field の full JSON frame は約 8.7 MB、主要表示 field
（`height`, `lake_depth`, `plate_id`, `river_flux`, `river_next`, `mantle_heat`）だけでも
約 1.3--1.6 MB ある。全 field の full frame を全 tick・全 keyframe について常時送る方式は、
初期転送量とブラウザ memory の両方が大きすぎる。

また precompute server の session cursor は seed ごとに共有されている。複数のブラウザが同じ公開 seed を
開くと、一方の seek が他方の field response を変え得るため、非同期 prefetch を追加する前に session を
world ごとに分離する必要がある。

## Decision

- `GET /api/worlds/:world_id/stream` を WebSocket endpoint とする。
- browser は中心 tick、近傍半径、既に保持する exact tick、粗い keyframe 間隔を subscribe message で送る。
- 近傍 window は、既知の連続 frame を再利用できなければ window 先頭の full anchor を1つ送り、以後を保存済み
  delta として順に送る。browser は delta を適用して各 exact tick を bounded cache に復元する。
- window が前進するときは、保持済み末尾から先の delta だけを送る。後退または遠方移動で連続 base が無ければ
  新しい anchor から再開する。
- 遠方 jump 用 keyframe は 256 tick 間隔を既定とし、主要表示 field だけを送る。browser は現在の完全 frame に
  keyframe の主要 field を重ねた preview を即座に表示し、通常 HTTP seek で得た exact frame に差し替える。
- WebSocket は read-only とする。session cursor の確定は従来の HTTP seek/advance が担う。
- WebSocket が使えない環境では従来の HTTP 経路へ自動的に fallback する。
- browser cache は exact 8 frame、coarse 16 frame を上限とする。再接続時は cache を利用しつつ subscribe を再送する。
- full world 同期の独立した field request は並列化し、fallback と exact 差し替えの RTT 累積を除く。
- `POST /api/worlds` は呼び出しごとに独立した world session を作る。同じ seed の store data は共有するが、cursor は共有しない。

## Trade-off

- JSON WebSocket は既存 DTO とデバッグ容易性を優先する。binary framing や WebSocket 圧縮は今回の範囲外であり、
  初回 exact anchor の転送量は残る。
- coarse preview では主要 field 以外が直前 frame の値であり、exact frame 到着まで統計・選択 overlay が一時的に古い。
  UI は preview を最終状態として確定せず、必ず exact 同期を行う。
- exact cache は full typed array を複数保持して latency を優先する。上限を固定し、長時間再生で memory が増え続けないようにする。
- store materialize は blocking worker へ逃がすが、JSON encode と送信は connection task で順次行う。encode の占有が
  計測で問題になった場合は binary payload を別 decision で検討する。

## Validation

- Rust の stream plan test 9件と Web cache/client test 4件が通過した。
- default と `precompute_server` feature の `cargo build`、Vite production build、Web lint が通過した。
- level 6 `alpha` store と Vite proxy を使い、`catalog`、tick 57 の `exact_anchor`、`complete` を受信した。
- 同じ seed から独立した2 session が作られ、後の初期化後も先の session が参照できることを確認した。
