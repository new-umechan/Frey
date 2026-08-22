# Pages 自動デプロイと公開 precompute API を使う UI 確認

## Status

Accepted

## Context

Cloudflare Pages の demo は手動の `workflow_dispatch` でのみ更新でき、毎回 project 名と API URL を入力する必要がある。また、`pnpm dev:wasm` はローカル WASM で simulation を実行するため、事前計算データが無い状態で UI だけを確認する用途には重い。

公開済みの `precompute_server` は HTTP engine として同じ UI contract を提供する。Vite の `/api` proxy を経由すれば、ローカル開発 origin を API の CORS allowlist に追加せず、この公開 API を UI の確認に再利用できる。

## Decision

- `main` への push のうち Web build に影響する変更を `Deploy Pages Demo` のトリガーに追加する。
- Pages project 名と API base URL は GitHub Actions repository variables の `FREY_PAGES_PROJECT` と `FREY_PUBLIC_API_BASE` から読む。手動実行では入力値で上書きできる。
- `pnpm dev:remote` を追加する。これはローカルの precompute/WASM build を行わず、`FREY_REMOTE_API_BASE` の公開 API を Vite proxy 経由で使う。

## Trade-off

自動デプロイは `main` の変更を直ちに公開する。公開 API の URL と Pages project は workflow file に固定せず repository variables に置くため、設定漏れ時は deploy を fail-fast させる。`dev:remote` は公開済みの read-only precompute data を表示する用途であり、Rust/WASM の simulation 変更や未公開データの検証には使えない。
