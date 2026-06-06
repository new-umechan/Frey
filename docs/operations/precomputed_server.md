# Precomputed Server

## 事前計算

```bash
pnpm precompute:world -- --seed alpha --ticks 1600 --out-dir data/precomputed/worlds --keyframe-interval 64
```

開発中にフル tick を生成する場合は release profile を使う。

```bash
pnpm precompute:world:release -- --seed alpha --ticks 1600 --out-dir data/precomputed/worlds --keyframe-interval 64
```

`precompute_world` は同じ seed を再生成する前に `out-dir/<seed>` を削除する。古い `.bin` と新しい `.bin.zst` が混ざらないようにするため。

既定では frame を `zstd` 圧縮して `.bin.zst` として保存する。未圧縮で生成したい場合:

```bash
pnpm precompute:world -- --seed alpha --ticks 1600 --out-dir data/precomputed/worlds --keyframe-interval 64 --compression none
```

検証用に小さい tick 数で生成する場合:

```bash
pnpm precompute:world -- --seed alpha --ticks 16 --out-dir .cache/frey/precompute-test --keyframe-interval 8
```

## 起動

```bash
FREY_PRECOMPUTE_STORE_DIR=data/precomputed/worlds pnpm server:precompute
```

既定値:

- `FREY_PRECOMPUTE_BIND=127.0.0.1:8787`
- `FREY_PRECOMPUTE_STORE_DIR=data/precomputed/worlds`
- `precompute_world --compression zstd`
- `precompute_world --retention-ticks 2`

`--retention-ticks` は precompute CLI 実行中に保持する checkpoint/undo log 数を指定する。precompute は前方に進みながら keyframe/delta を書く用途のため、既定では UI 用 timeline より短く保持する。

## Web 側の接続

開発時は precompute server と Vite をまとめて起動できる。

```bash
pnpm run dev
```

既定では次を使う。

- `FREY_PRECOMPUTE_BIND=127.0.0.1:8787`
- `FREY_PRECOMPUTE_STORE_DIR=data/precomputed/worlds`
- `VITE_FREY_ENGINE=http`
- `VITE_FREY_API_BASE=` (relative `/api`)
- Vite proxy target: `http://127.0.0.1:8787`
- Vite: `127.0.0.1:5173`

SSH 越しに見る場合は、手元の machine から `5173` を転送して `http://127.0.0.1:5173/` を開く。ブラウザは relative `/api` を叩き、Ubuntu 側の Vite dev server が `127.0.0.1:8787` へ proxy するため、通常は `8787` をブラウザへ転送しなくてもよい。

従来の WASM worker 経路で起動する場合:

```bash
pnpm run dev:wasm
```

個別に Vite app を起動する場合:

```bash
VITE_FREY_ENGINE=http VITE_FREY_API_BASE=http://127.0.0.1:8787 pnpm app:dev
```

`VITE_FREY_ENGINE` を指定しない場合は従来の WASM worker 経路を使う。

## 公開デモ

公開デモでは、Web 側の seed selector だけでなく server 側でも公開範囲を制限する。

```bash
FREY_PRECOMPUTE_BIND=127.0.0.1:8787 \
FREY_PRECOMPUTE_STORE_DIR=data/precomputed/worlds \
FREY_PUBLIC_SEEDS=alpha,beta,gamma \
FREY_PUBLIC_MESH_LEVEL=6 \
FREY_MAX_MESH_LEVEL=6 \
FREY_MAX_TICK=1600 \
FREY_MAX_LOD=2 \
FREY_DISABLE_PRECOMPUTE_REQUESTS=true \
FREY_CORS_ORIGINS=https://frey-demo.example.com \
pnpm server:precompute
```

公開用 env:

- `FREY_PUBLIC_SEEDS`: comma-separated allowlist。未指定なら任意 seed を許可する。
- `FREY_PUBLIC_MESH_LEVEL`: 指定した mesh level 以外を拒否する。
- `FREY_MAX_MESH_LEVEL`: `GET /api/mesh/:level` などの最大 mesh level。
- `FREY_MAX_TICK`: session の公開 head tick と seek 上限。
- `FREY_MAX_LOD`: `GET /api/worlds/:world_id/field/:field_kind?lod=` の最大 LOD。
- `FREY_DISABLE_PRECOMPUTE_REQUESTS=true`: 未計算 seed の queue 作成を拒否する。
- `FREY_CORS_ORIGINS`: comma-separated origin allowlist。未指定なら開発用の permissive CORS。

Cloudflare Tunnel などの reverse proxy 経由で公開する場合も、`precompute_server` は
`127.0.0.1` または private network に bind し、直接 internet へ公開しない。

Web 側で公開 seed selector を使う場合は、ビルド時に seed manifest を指定する。

```bash
VITE_FREY_ENGINE=http \
VITE_FREY_API_BASE=https://frey-api.example.com \
VITE_FREY_DEMO_SEEDS_URL=/demo-seeds.json \
pnpm build
```

`web/public/demo-seeds.json` は公開 demo の seed catalog で、先頭の seed が初期 world になる。
`VITE_FREY_DEMO_SEEDS_URL` を指定しない場合は従来通り任意 seed 入力を表示する。

## mameta systemd 運用

`precompute_server` は生成途中の store を読む前提ではない。公開中の server が参照する
store とは別の directory に precompute し、生成完了後に symlink を差し替えて server を
restart する。

推奨 layout:

```text
data/precomputed/worlds-active -> data/precomputed/releases/260605-alpha
data/precomputed/releases/<release-name>/
```

初回の active store 作成:

```bash
mkdir -p data/precomputed/releases
pnpm precompute:world:release -- --seed alpha --ticks 1600 --out-dir data/precomputed/releases/260605-alpha --keyframe-interval 64
tools/deploy/activate-precomputed-store.sh data/precomputed/releases/260605-alpha data/precomputed/worlds-active
```

systemd user service の登録:

```bash
mkdir -p ~/.config/systemd/user ~/.config/frey
cp ops/systemd/frey-precompute-server.service ~/.config/systemd/user/
cp ops/systemd/precompute-server.env.example ~/.config/frey/precompute-server.env
systemctl --user daemon-reload
systemctl --user enable --now frey-precompute-server
```

`~/.config/frey/precompute-server.env` の `FREY_PRECOMPUTE_STORE_DIR` は
`/home/ume/prog/Frey/data/precomputed/worlds-active` のような active symlink を指す。
`FREY_CORS_ORIGINS` は Cloudflare Pages の公開 URL に合わせる。

更新手順:

```bash
pnpm precompute:world:release -- --seed alpha --ticks 1600 --out-dir data/precomputed/releases/<new-release> --keyframe-interval 64
tools/deploy/activate-precomputed-store.sh data/precomputed/releases/<new-release> data/precomputed/worlds-active
systemctl --user restart frey-precompute-server
systemctl --user status frey-precompute-server
```

`precompute_world` は同じ out-dir の seed directory を削除して再生成するため、server が読んでいる
active store を out-dir に指定しない。

ログ確認:

```bash
journalctl --user -u frey-precompute-server -n 80
```

health check:

```bash
curl http://127.0.0.1:8787/api/health
```

## Cloudflare Tunnel 固定化

一時確認だけなら次でよい。

```bash
cloudflared tunnel --url http://127.0.0.1:8787
```

この URL は固定ではないため、公開運用では named tunnel と hostname route を使う。
mameta で初回だけ Cloudflare に login する。

```bash
cloudflared tunnel login
```

ブラウザで Cloudflare アカウントと zone を選ぶと `~/.cloudflared/cert.pem` が作られる。
次に tunnel を作る。

```bash
cloudflared tunnel create frey-api
```

出力された credentials path を `ops/cloudflared/frey-api-tunnel.yml.example` の
`credentials-file` に合わせて、実 config を `~/.cloudflared/frey-api.yml` として作る。
`hostname` は Cloudflare 管理下の実 hostname に置き換える。

DNS route:

```bash
cloudflared tunnel route dns frey-api frey-api.example.com
```

手動確認:

```bash
cloudflared tunnel --config ~/.cloudflared/frey-api.yml run frey-api
curl https://frey-api.example.com/api/health
```

Cloudflare Tunnel を service 化する方法は環境により異なる。まずは named tunnel の手動 run で
API が安定して見えることを確認してから、`cloudflared service install` または systemd user
service へ移す。

## 未計算 seed

固定一覧にない seed を入力すると API は生成リクエストを queue に記録し、ブラウザは「事前計算待ち」と表示する。現行版は queue を永続化せず、バックグラウンド生成 worker も起動しない。

公開デモで `FREY_DISABLE_PRECOMPUTE_REQUESTS=true` を指定した場合、未計算 seed または
許可されていない mesh level は queue に入れず `403 Forbidden` を返す。
