# benches/raw 再取得ステータス（2026-03-27）

`benches/raw` の全消しを避けるため、再取得容易分と手動依存分を分けて運用する。

## 再取得容易（削除候補）

- ERA5 monthly zip  
  - ファイル: `benches/raw/climate/era5_land_monthly_1970_2000.zip`
  - 再取得: `pnpm bench:fetch:era5`
- ERA5 annual nc（zip から再生成）
  - ファイル: `benches/raw/climate/era5_land_annual_1970_2000.nc`
  - 再生成: `pnpm bench:prepare:era5`
- GloFAS raw 群
  - ディレクトリ: `benches/raw/hydrology/glofas_raw/`
  - 再取得: `pnpm bench:fetch:glofas`
- GloFAS annual nc（raw から再生成）
  - ファイル: `benches/raw/hydrology/glofas_era5_annual_mean.nc`
  - 再生成: `pnpm bench:prepare:glofas`
- SoilGrids 出力
  - ディレクトリ: `benches/raw/ecology/soilgrids/`
  - 再生成: `pnpm bench:prepare:soilgrids:0p1deg`

## 手動依存（保持推奨）

- WorldClim 月次/年次 tif
- `benches/raw/climate/ai_et0.tif`（Aridity）
- `benches/raw/geology/ETOPO_2022_v1_60s_N90W180_surface.tif`
- `benches/raw/hydrology/HydroLAKES_polys_v10.*`
- `benches/raw/ecology/MOD44B/`, `benches/raw/ecology/MCD12Q1/` および canonical tif 群

## 削除コマンド

```bash
pnpm bench:raw:prune:recoverable:dry-run
pnpm bench:raw:prune:recoverable
```

実体は `benches/scripts/prune-recoverable-raw.sh`。圧縮には依存しない。
