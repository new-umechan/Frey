#![cfg(feature = "wasm_transport")]

//! セル単位の因果説明 (`explain_cell`)。
//!
//! クリックした 1 セルの、あるターゲット量が「なぜその値なのか」を、
//! ノード + 有向辺 + 寄与率の因果グラフとして返す。
//!
//! 設計原則: グラフはシミュレーションの計算式の投影であり、別に書き起こした
//! 物語ではない。寄与率 (`contribution_pct`) は式から厳密に配分できる箇所だけ
//! に入れ、非線形で厳密配分できない駆動要因は偏差 (`anomaly`) だけを示す。
//!
//! ## aridity (乾燥度) の因果
//!
//! `climate/surface.rs` の定義そのまま:
//!
//! ```text
//! aridity = atmospheric_demand / precipitation
//! ```
//!
//! したがって乾燥度の第一分解は demand と precipitation の 2 項に厳密に割れる。
//! 保存されている aridity と precipitation から demand を復元し
//! (`demand = aridity * precipitation`)、対数偏差を加法分解して寄与率にする。
//! demand をさらに駆動する気温・内陸度・緯度は、`atmospheric_evaporative_demand_mm`
//! の中で非線形に結合するため、ここでは偏差だけを示し寄与率は付けない。

use crate::application::world_dto::{ExplainCellResponse, ExplainEdge, ExplainNode};
use crate::application::world_service::WorldService;
use crate::sim::world::Biome;

fn world_not_found_error(world_id: &str) -> String {
    format!("world not found: {world_id}")
}

/// 陸セル集合に対する平均・標準偏差。z-score 算出に使う。
struct LandStats {
    mean: f32,
    std: f32,
}

impl LandStats {
    fn from_values<'a>(values: impl Iterator<Item = f32>) -> Self {
        let mut count = 0u32;
        let mut sum = 0.0f64;
        let mut sum_sq = 0.0f64;
        for v in values {
            if v.is_finite() {
                count += 1;
                sum += v as f64;
                sum_sq += (v as f64) * (v as f64);
            }
        }
        if count == 0 {
            return LandStats {
                mean: 0.0,
                std: 1.0,
            };
        }
        let mean = sum / count as f64;
        let variance = (sum_sq / count as f64 - mean * mean).max(0.0);
        LandStats {
            mean: mean as f32,
            std: (variance.sqrt() as f32).max(1e-6),
        }
    }

    fn zscore(&self, value: f32) -> f32 {
        (value - self.mean) / self.std
    }
}

fn biome_label(biome: Biome) -> &'static str {
    match biome {
        Biome::TropicalForest => "熱帯林",
        Biome::Savanna => "サバンナ",
        Biome::Desert => "砂漠",
        Biome::Grassland => "草原",
        Biome::TemperateForest => "温帯林",
        Biome::BorealForest => "亜寒帯林",
        Biome::Tundra => "ツンドラ",
        Biome::Wetland => "湿地",
        Biome::Alpine => "高山",
    }
}

/// 因果グラフの入力。公開フィールドの束。
///
/// ライブ経路は `World` から、公開経路(precompute サーバー)は事前計算フレームから、
/// 同じ形で組み立てて同じ説明関数へ渡す。説明ロジックはこの束だけに依存し、ライブ
/// 実行時の内部状態には触れない。
/// 参照: docs/decisions/260724-causal-graph-over-published-fields.md
pub(crate) struct ExplainInputs<'a> {
    pub sea_level: f32,
    pub height: &'a [f32],
    pub aridity: &'a [f32],
    pub precipitation: &'a [f32],
    pub temperature: &'a [f32],
    pub distance_from_ocean: &'a [f32],
    pub latitude: &'a [f32],
    pub biome: &'a [Biome],
}

impl ExplainInputs<'_> {
    fn cell_count(&self) -> usize {
        self.height.len()
    }
}

/// ライブ経路のアダプタ。`World` から `ExplainInputs` を束ねて説明本体へ渡す。
pub(crate) fn explain_cell(
    service: &WorldService,
    world_id: &str,
    cell_index: u32,
    target: &str,
) -> Result<ExplainCellResponse, String> {
    let managed = service
        .world(world_id)
        .ok_or_else(|| world_not_found_error(world_id))?;
    let world = &managed.world;

    let inputs = ExplainInputs {
        sea_level: world.sea_level_offset(),
        height: &world.state.geology.height,
        aridity: &world.state.climate.aridity,
        precipitation: &world.state.climate.precipitation,
        temperature: &world.state.climate.temperature,
        distance_from_ocean: world.distance_from_ocean_values(),
        latitude: &world.projections.terrain.latitude,
        biome: &world.state.ecology.biome,
    };

    explain_from_inputs(&inputs, cell_index, target)
}

/// 公開経路とライブ経路で共有する説明本体。`ExplainInputs` だけに依存する純粋関数。
pub(crate) fn explain_from_inputs(
    inputs: &ExplainInputs<'_>,
    cell_index: u32,
    target: &str,
) -> Result<ExplainCellResponse, String> {
    let i = cell_index as usize;
    let cell_count = inputs.cell_count();
    if i >= cell_count {
        return Err(format!(
            "cell index out of range: {cell_index} (cell_count={cell_count})"
        ));
    }

    match target {
        "aridity" => explain_aridity(inputs, cell_index),
        other => Err(format!("unsupported explain target: {other}")),
    }
}

fn explain_aridity(
    inputs: &ExplainInputs<'_>,
    cell_index: u32,
) -> Result<ExplainCellResponse, String> {
    let i = cell_index as usize;
    let sea_level = inputs.sea_level;
    let is_land = inputs.height[i] > sea_level;

    let aridity = inputs.aridity.get(i).copied().unwrap_or(0.0);
    let precip = inputs.precipitation.get(i).copied().unwrap_or(0.0);
    let temperature = inputs.temperature.get(i).copied().unwrap_or(0.0);
    let distance_from_ocean = inputs.distance_from_ocean.get(i).copied().unwrap_or(0.0);
    let latitude = inputs.latitude.get(i).copied().unwrap_or(0.0);
    let biome = inputs.biome.get(i).copied().unwrap_or(Biome::Desert);

    if !is_land {
        return Ok(ExplainCellResponse {
            cell_index,
            target: "aridity".to_string(),
            target_value: aridity,
            target_label: "海洋".to_string(),
            is_land: false,
            nodes: Vec::new(),
            edges: Vec::new(),
            summary: "ここは海のため、乾燥度は定義されない。".to_string(),
        });
    }

    // demand = aridity * precipitation (climate/surface.rs の定義の逆算)。
    let demand = aridity * precip;

    // 陸セルの基準統計。対数偏差の基準に幾何平均を使うため precip/demand は ln で集計。
    let heights = inputs.height;
    let land_mask = |idx: usize| heights.get(idx).copied().unwrap_or(f32::MIN) > sea_level;

    let precip_field = inputs.precipitation;
    let aridity_field = inputs.aridity;
    let temp_field = inputs.temperature;
    let dist_field = inputs.distance_from_ocean;

    let ln_precip_stats = LandStats::from_values(
        (0..precip_field.len())
            .filter(|&idx| land_mask(idx) && precip_field[idx] > 0.0)
            .map(|idx| precip_field[idx].ln()),
    );
    let ln_demand_stats = LandStats::from_values(
        (0..aridity_field.len())
            .filter(|&idx| land_mask(idx) && precip_field.get(idx).copied().unwrap_or(0.0) > 0.0)
            .map(|idx| (aridity_field[idx] * precip_field[idx]).max(1e-6).ln()),
    );
    let temp_stats = LandStats::from_values(
        (0..temp_field.len())
            .filter(|&idx| land_mask(idx))
            .map(|idx| temp_field[idx]),
    );
    let dist_stats = LandStats::from_values(
        (0..dist_field.len())
            .filter(|&idx| land_mask(idx))
            .map(|idx| dist_field[idx]),
    );

    // aridity = demand / precip なので
    //   ln(aridity/aridity0) = ln(demand/demand0) - ln(precip/precip0)
    // 右辺の 2 項が、乾燥度の対数偏差への demand 寄与 / precip 不足寄与。
    let ln_demand_anom = demand.max(1e-6).ln() - ln_demand_stats.mean;
    let ln_precip_anom = precip.max(1e-6).ln() - ln_precip_stats.mean;
    let demand_contrib = ln_demand_anom; // 正: 乾燥へ押す
    let precip_contrib = -ln_precip_anom; // 正(降水が平均以下): 乾燥へ押す

    let magnitude = demand_contrib.abs() + precip_contrib.abs();
    let (demand_pct, precip_pct) = if magnitude > 1e-6 {
        (
            (demand_contrib.abs() / magnitude) * 100.0,
            (precip_contrib.abs() / magnitude) * 100.0,
        )
    } else {
        (50.0, 50.0)
    };

    let nodes = vec![
        ExplainNode {
            id: "aridity".to_string(),
            label: "乾燥度".to_string(),
            value: aridity,
            unit: "demand/precip".to_string(),
            anomaly: LandStats {
                mean: ln_demand_stats.mean - ln_precip_stats.mean,
                std: (ln_demand_stats.std + ln_precip_stats.std).max(1e-6),
            }
            .zscore(aridity.max(1e-6).ln()),
            contribution_pct: None,
        },
        ExplainNode {
            id: "demand".to_string(),
            label: "大気の蒸発要求".to_string(),
            value: demand,
            unit: "mm".to_string(),
            anomaly: ln_demand_stats.zscore(demand.max(1e-6).ln()),
            contribution_pct: Some(demand_pct),
        },
        ExplainNode {
            id: "precipitation".to_string(),
            label: "降水量".to_string(),
            value: precip,
            unit: "mm".to_string(),
            anomaly: ln_precip_stats.zscore(precip.max(1e-6).ln()),
            contribution_pct: Some(precip_pct),
        },
        ExplainNode {
            id: "temperature".to_string(),
            label: "気温".to_string(),
            value: temperature,
            unit: "℃".to_string(),
            anomaly: temp_stats.zscore(temperature),
            contribution_pct: None,
        },
        ExplainNode {
            id: "distance_from_ocean".to_string(),
            label: "内陸度".to_string(),
            value: distance_from_ocean,
            unit: "cells".to_string(),
            anomaly: dist_stats.zscore(distance_from_ocean),
            contribution_pct: None,
        },
        ExplainNode {
            id: "latitude".to_string(),
            label: "緯度".to_string(),
            value: latitude,
            unit: "°".to_string(),
            // 亜熱帯高圧帯 (|緯度|~25°) への近さを乾燥駆動として符号化。
            anomaly: 1.0 - ((latitude.abs() - 25.0).abs() / 25.0).clamp(0.0, 1.0),
            contribution_pct: None,
        },
    ];

    let edges = vec![
        ExplainEdge {
            from: "demand".to_string(),
            to: "aridity".to_string(),
            sign: 1,
            contribution_pct: Some(demand_pct),
        },
        ExplainEdge {
            from: "precipitation".to_string(),
            to: "aridity".to_string(),
            sign: -1,
            contribution_pct: Some(precip_pct),
        },
        ExplainEdge {
            from: "temperature".to_string(),
            to: "demand".to_string(),
            sign: 1,
            contribution_pct: None,
        },
        ExplainEdge {
            from: "distance_from_ocean".to_string(),
            to: "demand".to_string(),
            sign: 1,
            contribution_pct: None,
        },
        ExplainEdge {
            from: "latitude".to_string(),
            to: "demand".to_string(),
            sign: 1,
            contribution_pct: None,
        },
    ];

    let dominant = if demand_pct >= precip_pct {
        "高い蒸発要求"
    } else {
        "少ない降水"
    };
    let arid_word = if aridity >= 1.0 { "乾燥" } else { "湿潤" };
    let summary = format!(
        "この地点は{biome}({arid_word}度 {aridity:.2})。主因は{dominant}(蒸発要求 {demand_pct:.0}% / 降水 {precip_pct:.0}%)。",
        biome = biome_label(biome),
    );

    Ok(ExplainCellResponse {
        cell_index,
        target: "aridity".to_string(),
        target_value: aridity,
        target_label: biome_label(biome).to_string(),
        is_land: true,
        nodes,
        edges,
        summary,
    })
}
