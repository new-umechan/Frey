#![cfg(feature = "wasm_transport")]

//! 地点単位の因果ストーリー (`explain_cell`)。
//!
//! クリックした 1 地点が「なぜ今の姿なのか」を、分野をまたいだ原因の連鎖として返す。
//! 起点は植生 (biome)。植生の種類を決めた要因から上流(気温なら標高・緯度)へたどる。
//!
//! 設計原則: グラフはモデルの依存関係の投影であり、物語のために創作した説明ではない。
//! エッジは実コードの依存に一致させ、公開フィールドだけから組む(ライブ実行状態に
//! 依存しない)。
//! 参照: docs/decisions/260724-causal-story-cross-module-trace.md
//!
//! ## 植生ストーリーの依存(実コード)
//!
//! - 植生の種類は `標高・気温・降水量・河川流量・樹冠` の閾値カスケードで決まる
//!   (`sim/ecology/mod.rs` の `classify_biome` / `biome_decisive_factor`)。
//! - 気温は `緯度で決まる基準温度 − 標高 × 逓減率`(`sim/climate/surface.rs`)。
//! - 標高は地形(プレート)由来。

use crate::application::world_dto::{ExplainCellResponse, ExplainEdge, ExplainNode};
use crate::application::world_service::WorldService;
use crate::sim::ecology::{biome_decisive_factor, BiomeFactor};
use crate::sim::world::Biome;

fn world_not_found_error(world_id: &str) -> String {
    format!("world not found: {world_id}")
}

/// 因果ストーリーの入力。公開フィールドの束。
///
/// ライブ経路は `World` から、公開経路は事前計算フレームから、同じ形で組み立てて
/// 同じ説明関数へ渡す。
pub(crate) struct ExplainInputs<'a> {
    pub sea_level: f32,
    pub height: &'a [f32],
    pub temperature: &'a [f32],
    pub precipitation: &'a [f32],
    pub river_flow: &'a [f32],
    pub tree_cover: &'a [f32],
    pub latitude: &'a [f32],
    pub biome: &'a [Biome],
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

fn factor_label(factor: BiomeFactor) -> &'static str {
    match factor {
        BiomeFactor::Elevation => "標高",
        BiomeFactor::Temperature => "気温",
        BiomeFactor::Precipitation => "降水量",
        BiomeFactor::Flooding => "河川",
    }
}

fn node(
    id: &str,
    label: &str,
    module: &str,
    value: f32,
    unit: &str,
    decisive: bool,
) -> ExplainNode {
    ExplainNode {
        id: id.to_string(),
        label: label.to_string(),
        module: module.to_string(),
        value,
        unit: unit.to_string(),
        decisive,
    }
}

fn edge(from: &str, to: &str, decisive: bool) -> ExplainEdge {
    ExplainEdge {
        from: from.to_string(),
        to: to.to_string(),
        decisive,
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
        temperature: &world.state.climate.temperature,
        precipitation: &world.state.climate.precipitation,
        river_flow: &world.state.hydrology.river_flow,
        tree_cover: &world.state.ecology.tree_cover,
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
    let cell_count = inputs.height.len();
    if i >= cell_count {
        return Err(format!(
            "cell index out of range: {cell_index} (cell_count={cell_count})"
        ));
    }

    match target {
        // 現状は植生を起点とするストーリーのみ。クリックはこれを呼ぶ。
        "biome" => explain_biome(inputs, cell_index),
        other => Err(format!("unsupported explain target: {other}")),
    }
}

fn explain_biome(
    inputs: &ExplainInputs<'_>,
    cell_index: u32,
) -> Result<ExplainCellResponse, String> {
    let i = cell_index as usize;
    let height = inputs.height.get(i).copied().unwrap_or(0.0);
    let is_land = height > inputs.sea_level;
    let biome = inputs.biome.get(i).copied().unwrap_or(Biome::Desert);

    if !is_land {
        return Ok(ExplainCellResponse {
            cell_index,
            headline: "海".to_string(),
            summary: "海のため植生はない。".to_string(),
            is_land: false,
            nodes: Vec::new(),
            edges: Vec::new(),
        });
    }

    let temperature = inputs.temperature.get(i).copied().unwrap_or(0.0);
    let precipitation = inputs.precipitation.get(i).copied().unwrap_or(0.0);
    let river_flow = inputs.river_flow.get(i).copied().unwrap_or(0.0);
    let tree_cover = inputs.tree_cover.get(i).copied().unwrap_or(0.0);
    let latitude = inputs.latitude.get(i).copied().unwrap_or(0.0);
    let max_flow = inputs.river_flow.iter().copied().fold(0.0_f32, f32::max);

    let factor = biome_decisive_factor(
        tree_cover,
        temperature,
        precipitation,
        river_flow,
        height,
        max_flow,
    );

    // 起点(いま見えている姿)。
    let mut nodes = vec![node("biome", biome_label(biome), "植生", 0.0, "", false)];
    let mut edges = Vec::new();

    // 決め手の要因から上流へ辿る。同じ地点で辿れる範囲のみ(空間をまたぐ雨陰・河川の
    // 上流は次段の拡張で扱う)。
    match factor {
        BiomeFactor::Precipitation => {
            nodes.push(node(
                "precipitation",
                "降水量",
                "気候",
                precipitation,
                "mm",
                true,
            ));
            edges.push(edge("precipitation", "biome", true));
        }
        BiomeFactor::Temperature => {
            nodes.push(node("temperature", "気温", "気候", temperature, "℃", true));
            edges.push(edge("temperature", "biome", true));
            // 気温の上流(地形): 基準温度は緯度、逓減は標高。
            nodes.push(node("height", "標高", "地形", height, "", false));
            nodes.push(node("latitude", "緯度", "地形", latitude, "°", false));
            edges.push(edge("height", "temperature", false));
            edges.push(edge("latitude", "temperature", false));
        }
        BiomeFactor::Elevation => {
            nodes.push(node("height", "標高", "地形", height, "", true));
            edges.push(edge("height", "biome", true));
        }
        BiomeFactor::Flooding => {
            nodes.push(node("river_flow", "河川流量", "水文", river_flow, "", true));
            edges.push(edge("river_flow", "biome", true));
        }
    }

    let summary = format!(
        "{}。主な要因は{}。",
        biome_label(biome),
        factor_label(factor)
    );

    Ok(ExplainCellResponse {
        cell_index,
        headline: biome_label(biome).to_string(),
        summary,
        is_land: true,
        nodes,
        edges,
    })
}
