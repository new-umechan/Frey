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

## 未計算 seed

固定一覧にない seed を入力すると API は生成リクエストを queue に記録し、ブラウザは「事前計算待ち」と表示する。現行版は queue を永続化せず、バックグラウンド生成 worker も起動しない。
