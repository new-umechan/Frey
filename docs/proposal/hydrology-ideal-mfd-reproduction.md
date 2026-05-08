# Hydrology理想MFD再現計画

## Status

Draft

## 背景

- `docs/reference/modules/hydrology.md` では、`HydrologyMFDSystem` は「複数流下先 + 分配率」の理想MFDを採用する仕様になっている。
- 一方、現行実装は `river_next`（単一流下先）を主系として扱い、`river_downstream` は `rebuild_mfd_from_primary` で 1.0 重みの単一edgeへ再構築している。
- そのため、仕様上は MFD だが、実挙動は実質 SFD/D8 的な主流路モデルに寄っており、デルタ・扇状地・緩斜面での分散流を十分に再現できていない。

## 目的

- `river_next` 主系依存を解消し、研究知見に整合する MFD（分配流）を Hydrology の正本として再現する。
- sink / fill-spill の既存安定性を維持したまま、流量・侵食・堆積の下流伝播を「分配率つき流路網」で計算する。
- ベンチマーク指標（`river_flow` 相関、`is_lake` F1、診断値）で劣化を防ぎつつ、理想MFD仕様との乖離を埋める。

## 提案概要

1. `river_downstream`（CSR: offsets/cells/weights）を Hydrology の流路正本に格上げする。
2. `river_next` は互換用途の派生ビューへ降格する（必要箇所は「最大重み edge」または「sink/spill 代表edge」から導出）。
3. 流量累積は `flow_flux_on_primary_network` を置換し、`fraction_i ∝ slope_i^x` に基づく重みで上流寄与を分配する。
4. Holmgren 系の指数（勾配依存の可変指数）を導入し、急斜面で集中・緩斜面で分散する挙動を再現する。
5. `river_downstream` の分岐保持は容量4を既定とし、候補計算後に上位4本へ刈り込んで再正規化する。
6. depression handling は既存の fill-spill/sink 正本を維持し、overflow edge は単一spill（`weight=1.0`）を既定にする。
7. 段階移行として、初期は feature flag で切替可能にし、seed regression と hydrology_solo で比較しながら既定値化する。

## スコープ

この proposal で決めること:

- Hydrology の流路正本を `river_downstream` 側へ寄せる方針
- MFD 分配則（勾配べき乗・可変指数）を研究準拠で採用する方針
- 可変指数は固定値近似ではなく勾配依存（`x = clamp(a*|∇h| + b, x_min, x_max)`）を既定にすること
- `river_downstream` の最大分岐数を4とし、上位重み保持 + 再正規化を既定にすること
- fill-spill overflow edge は単一spill維持（spill先へのMFD再分配はしない）を既定にすること
- `river_next` を互換ビューにする移行方針
- 検証を `hydrology_solo` + seed gate の既存運用に接続する方針

この proposal でまだ決めないこと:

- 可変指数パラメータの最終値（`a`, `b`）
- `river_next` を参照する全下流モジュールの最終API形
- v2 以降の depression hierarchy（fill-spill-merge）完全導入

## 決定事項

- 可変指数は勾配依存を既定とし、固定指数への簡略化は既定にしない。
- MFD 分岐は「候補全列挙 → 上位4本保持 → 重み再正規化」を既定にする。
- fill-spill の overflow は単一spill edge（`weight=1.0`）を既定にし、spill先の再分配は導入しない。

## 成功条件

- `rebuild_mfd_from_primary` 相当の単一edge再構築を正本計算経路から外せる。
- `river_flow` 算出が分配率つき流路で一貫して計算される。
- `hydrology_solo` で次を満たす:
  - `river_flow` Spearman が現行比で悪化しない（同等以上を目標）
  - `is_lake` F1 を維持する
  - `sediment_budget_ratio` 等の診断値に破綻がない
- seed gate で既存の安定性条件（`top10_river_flux_sum` 個別閾値を含む）を満たす。

## リスクとトレードオフ

- 分配流の導入で計算・メモリアクセスが増え、1tick コストは上がりうる。
- `river_next` 前提の処理が残ると二重管理期間が発生し、バグ混入面が増える。
- 可変指数の設定次第で、流路が過集中/過分散になり、侵食・堆積診断が不安定になる。

近似は許容するが、何を近似し、何を保存するかを明記したうえで導入する。

## 実施計画

1. `river_next` 参照箇所を棚卸しし、「正本」「互換」「削除候補」に分類する。
2. MFD 正本の流量累積（weighted accumulation）を実装し、`river_flow` を置換する。
3. Holmgren 系可変指数（勾配依存 + clamp）を実装し、分配率計算を `fraction ∝ slope^x` へ統一する。
4. MFD 分岐保持を上位4本へ制限し、刈り込み後の重み再正規化を実装する。
5. fill-spill overflow を単一spill edge として MFD 本体から責務分離して実装する。
6. `river_next` を派生ビュー化し、公開API・undo・query 層の整合を取る。
7. `hydrology_solo` と seed gate で比較評価し、閾値内で回帰を確認する。
8. 採用後に `docs/decisions/` と `docs/reference/modules/hydrology.md` を更新する。

## 参考研究

- Freeman, 1991: 格子DEMでの発散流を扱う MFD の基礎。
- Quinn et al., 1991: 勾配重み付き流向分配（MFD）による分散流表現。
- Holmgren, 1994: `slope^x` 形式で集中/分散を制御する流向分配。
- Qin et al., 2007: 地形条件に応じた適応的指数でのMFD改良。
- Barnes et al., 2014: Priority-Flood による depression handling（sink/lake処理の基盤）。
- Salles et al., 2018（goSPL）: sink 容量・spill を含む地形進化計算の実装系譜。
