# 世界地形シミュレーション再設計仕様

本ディレクトリは、次の4点セットを中核にした地形シミュレーションの数式仕様を定義する。

- プレート運動はオイラー回転の剛体運動で表す
- 鉛直速度Uは境界タイプ、薄板、アイソスタシーで表す
- 侵食はストリームパワーと拡散で表す
- 気候は風駆動循環から降水場を与えてKを変調する

## 読み順

1. `docs/core/redesign/01_common_foundation.md`
2. `docs/core/redesign/02_plate_rigid_rotation.md`
3. `docs/core/redesign/03_vertical_velocity_u.md`
4. `docs/core/redesign/04_topography_evolution_equation.md`
5. `docs/core/redesign/05_erosion_stream_power_diffusion.md`
6. `docs/core/redesign/06_climate_precip_k_modulation.md`
7. `docs/core/redesign/07_coupling_execution.md`
8. `docs/core/redesign/08_calibration_validation.md`

## 適用範囲

- 本仕様は数式設計を対象とする
- 公開APIとUI仕様は別ドキュメントで定義する
