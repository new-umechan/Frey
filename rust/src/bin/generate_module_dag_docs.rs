use std::env;
use std::fs;
use std::path::PathBuf;

fn default_output_path() -> PathBuf {
    PathBuf::from("docs/architecture/module_boundaries.generated.md")
}

fn markdown_escape(text: &str) -> String {
    text.replace('|', "\\|")
}

fn main() -> Result<(), String> {
    let output_path = env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(default_output_path);

    let modules = frey_wasm::sim::module_doc_records();
    let graph = frey_wasm::sim::module_graph_record();

    let mut lines: Vec<String> = Vec::new();
    lines.push("# Module Declaration DAG (Generated)".to_string());
    lines.push(String::new());
    lines.push("この文書は `rust/src/sim/exec/modules.rs` の宣言から自動生成される。".to_string());
    lines.push(String::new());
    lines.push("## Modules".to_string());
    lines.push(String::new());
    lines.push("| phase | module | inbox | profile | display | execution | tick_boundary | reads | writes | feedback | depends_on | description |".to_string());
    lines.push("| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |".to_string());

    for module in modules {
        lines.push(format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
            module.phase,
            module.module,
            module.inbox,
            module.profile,
            module.display,
            module.execution,
            if module.tick_boundary { "yes" } else { "no" },
            markdown_escape(&module.reads.join(", ")),
            markdown_escape(&module.writes.join(", ")),
            markdown_escape(&module.feedback_targets.join(", ")),
            markdown_escape(&module.depends_on.join(", ")),
            markdown_escape(module.description),
        ));
    }

    lines.push(String::new());
    lines.push("## Edges".to_string());
    lines.push(String::new());
    lines.push("| from_phase | from_module | to_phase | to_module |".to_string());
    lines.push("| --- | --- | --- | --- |".to_string());
    for edge in &graph.edges {
        lines.push(format!(
            "| {} | {} | {} | {} |",
            edge.from_phase, edge.from_module, edge.to_phase, edge.to_module
        ));
    }

    lines.push(String::new());
    lines.push(format!("module_count: {}", graph.modules.len()));
    lines.push(format!("edge_count: {}", graph.edges.len()));
    lines.push(String::new());

    let output = format!("{}\n", lines.join("\n"));
    fs::write(&output_path, output).map_err(|error| {
        format!(
            "failed to write generated module DAG docs to {}: {error}",
            output_path.display()
        )
    })?;
    println!("generated {}", output_path.display());
    Ok(())
}
