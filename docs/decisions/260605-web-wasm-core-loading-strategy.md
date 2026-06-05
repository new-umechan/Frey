# Web WASM コア読み込み戦略

## Status

Draft

## Context

Web 版は `EngineClient` を介して実行系を差し替えられる。
現状では `VITE_FREY_ENGINE=http` または `VITE_FREY_API_BASE` が設定されていれば HTTP precomputed engine を使い、未設定なら WASM worker を使う。

生成済み WASM は `generated/wasm/web/frey_wasm_bg.wasm` にまとまっており、現時点のサイズは次の通り。

- 未圧縮: 約 828KB
- gzip: 約 307KB
- JS glue: 約 33KB

WASM には `WorldSimController` によるシミュレーション実行 API だけでなく、`generate_mesh`、`generate_geology`、`build_render_positions` も同居している。
そのため、シミュレーションコアだけを Web 初期ロードから外す場合でも、WASM を完全削除するか、小さい用途に分割して残すかを別途判断する必要がある。

## Discussion

シミュレーションコアを Web 初期ロードから外せる場合、主に次を削減できる。

- 約 307KB gzip の WASM 転送
- WASM compile/instantiate
- worker 初期化
- WASM glue の一部

短縮幅は端末と回線に依存する。
目安として、高速な PC では数十 ms、一般的なモバイルや低速 CPU では 100-300ms 程度、低速回線や cold load では 300ms を超える可能性がある。

ただし、現段階では仕様が固まりきっていない。
この段階で「シミュレーションコアは消すが、WASM の一部は残す」と決めて分割設計を進めると、API 境界、ビルド、型生成、テスト対象が増える。
得られる利点はロード時間短縮だが、残す処理の責務がまだ変わる可能性があるため、WASM 分割は早すぎる最適化になりやすい。

## Current Direction

当面は次の順序を有力案とする。

1. Web の既定実行経路を HTTP/precomputed に寄せ、WASM を初期ロードから外す。
2. WASM は開発、ベンチ、ローカル完結実行用の fallback として残す。
3. 初期表示、世界読み込み、描画更新を実測する。
4. 実測で必要になった場合だけ、小さい WASM への分割を検討する。

この段階では、WASM に残す具体的な処理を採用判断しない。

## Candidate Residual WASM Responsibilities

将来 WASM を部分的に残す場合の候補は次の通り。

- `generate_mesh(level)`: 初期地形メッシュ生成
- `build_render_positions(input)`: 描画用 position buffer 生成
- 受信した field/delta の typed array 変換、圧縮展開
- terrain/overlay 用の派生値計算
- geometry/LOD helper

一方で、次はシミュレーションコアなので、Web 初期ロード削減の対象になりやすい。

- `WorldSimController.init_world`
- `WorldSimController.exec_world_slice`
- `WorldSimController.advance_timeline`
- `WorldSimController.get_view_delta`
- `WorldSimController.get_metrics`
- checkpoint/rewind/seek など timeline 操作

## Risks

WASM コアを Web 初期ロードから外すデメリットは次の通り。

- オフラインまたはローカル完結のシミュレーション実行が弱くなる。
- サーバ/API 依存になり、待ち時間、失敗、queued/pending 表示が必要になる。
- seed や config を変えた即時生成が遅くなる、または事前計算済みに制限される。
- Web とサーバの API contract、キャッシュ invalidation、結果差分の管理が増える。
- wasm lane のベンチやローカル検証の位置づけを整理する必要がある。
- `generate_mesh` など軽量処理まで同じ WASM に含まれているため、分割しない限り削減効果が限定される。

## Open Questions

- 公開 Web 版の既定 engine をいつ HTTP/precomputed に切り替えるか。
- HTTP/precomputed が unavailable のとき、WASM fallback を自動で使うか、明示的な開発モードに限定するか。
- 初期ロードの受け入れ基準を LCP、TTI、first terrain render のどれで見るか。
- 小さい visual/mesh 専用 WASM が必要になる実測閾値をどう置くか。
