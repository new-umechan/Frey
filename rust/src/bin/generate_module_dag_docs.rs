use std::env;
use std::fs;
use std::path::PathBuf;

fn default_output_path() -> PathBuf {
    PathBuf::from("docs/reference/architecture/module_boundaries.md")
}

fn generate_dag_content(
    modules: &[frey_wasm::sim::exec::ModuleDocRecord],
    graph: &frey_wasm::sim::exec::ModuleGraphRecord,
) -> String {
    let mut lines = vec![
        "## tick内依存（Declaration DAG）".to_string(),
        String::new(),
        "実行順は `ModuleDeclaration` の `reads` / `writes` / `feedback` から自動生成される。"
            .to_string(),
        "固定の hand-written DAG は正本にしない。".to_string(),
        "更新は `pnpm run module:docs` で行う。".to_string(),
        String::new(),
        "### Phase 実行順".to_string(),
        String::new(),
    ];

    let phase_order: Vec<&str> = modules.iter().map(|m| m.phase).collect();
    lines.push(phase_order.join(" → "));
    lines.push(String::new());

    lines.push("### 依存エッジ一覧".to_string());
    lines.push(String::new());
    lines.push("| from | to |".to_string());
    lines.push("| --- | --- |".to_string());
    let mut edges_by_from: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for edge in &graph.edges {
        let from_key = format!("{} ({})", edge.from_phase, edge.from_module);
        let to_val = format!("{} ({})", edge.to_phase, edge.to_module);
        edges_by_from.entry(from_key).or_default().push(to_val);
    }
    let mut sorted_keys: Vec<_> = edges_by_from.keys().cloned().collect();
    sorted_keys.sort();
    for from_key in &sorted_keys {
        let tos = &edges_by_from[from_key];
        let mut sorted_tos = tos.clone();
        sorted_tos.sort();
        lines.push(format!("| {} | {} |", from_key, sorted_tos.join(", ")));
    }

    lines.push(String::new());
    lines.push(format!("module_count: {}", graph.modules.len()));
    lines.push(format!("edge_count: {}", graph.edges.len()));
    lines.push(String::new());

    lines.join("\n")
}

fn main() -> Result<(), String> {
    let output_path = env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(default_output_path);

    let modules = frey_wasm::sim::module_doc_records();
    let graph = frey_wasm::sim::module_graph_record();

    let dag_content = generate_dag_content(&modules, &graph);

    let file_content = fs::read_to_string(&output_path)
        .map_err(|error| format!("failed to read {}: {error}", output_path.display()))?;

    let start_marker = "<!-- auto_generated_start -->";
    let end_marker = "<!-- auto_generated_end -->";

    let start_pos = file_content
        .find(start_marker)
        .ok_or_else(|| format!("start marker not found in {}", output_path.display()))?;
    let end_pos = file_content
        .find(end_marker)
        .ok_or_else(|| format!("end marker not found in {}", output_path.display()))?;

    if start_pos >= end_pos {
        return Err(format!(
            "marker order invalid: end marker appears before or at start marker position in {}",
            output_path.display()
        ));
    }

    let before = &file_content[..start_pos + start_marker.len()];
    let after = &file_content[end_pos..];

    let new_content = format!("{}\n{}\n{}", before, dag_content, after);

    fs::write(&output_path, new_content)
        .map_err(|error| format!("failed to write {}: {error}", output_path.display()))?;
    println!("updated {}", output_path.display());
    Ok(())
}
