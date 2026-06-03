# Precomputed Server

## 事前計算

```bash
pnpm precompute:world -- --seed alpha --ticks 1600 --out-dir data/precomputed/worlds --keyframe-interval 64
```

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

## Web 側の接続

別 terminal で次を指定して Vite app を起動する。

```bash
VITE_FREY_ENGINE=http VITE_FREY_API_BASE=http://127.0.0.1:8787 pnpm app:dev
```

`VITE_FREY_ENGINE` を指定しない場合は従来の WASM worker 経路を使う。

## 未計算 seed

固定一覧にない seed を入力すると API は生成リクエストを queue に記録し、ブラウザは「事前計算待ち」と表示する。現行版は queue を永続化せず、バックグラウンド生成 worker も起動しない。
