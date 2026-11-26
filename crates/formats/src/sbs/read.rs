use super::{constant::MAX_GRAPHS, placeholder::report_placeholders, sbs_read::SbsRead, sbs_reader::SbsReader, warnings::Warnings, xml::child_value};
use crate::Result;
use materialcraft_core::Package;
use roxmltree::Node as XmlNode;
use std::collections::{HashMap, HashSet};

pub fn read_sbs(bytes: &[u8], name: &str, linked: &dyn Fn(&str) -> Option<Vec<u8>>) -> Result<SbsRead> {
    super::SbsDocument::parse(bytes)?.read(name, linked)
}

pub(super) fn read_document(
    doc: &craft_sbs::PackageDocument<'_>,
    name: &str,
    linked: &dyn Fn(&str) -> Option<Vec<u8>>,
    resource_budget: Option<&std::cell::Cell<u64>>,
) -> Result<SbsRead> {
    let text = doc.text();
    let mut warnings = Warnings::default();
    let package_name = doc.package_name(name);

    // Include one probe byte so a valid inline asset can use the exact retained limit
    let inflation_work = std::cell::Cell::new(resource_budget.map_or(0, |budget| budget.get().saturating_add(1)));
    let mut reader = SbsReader {
        inflation_work,
        resource_budget,
        text,
        warnings: &mut warnings,
        dependencies: HashMap::new(),
        graph_names: HashMap::new(),
        resource_names: HashMap::new(),
        external_resource_names: HashMap::new(),
        direct_resource_names: HashMap::new(),
        used_resource_names: HashSet::new(),
        node_count: 0,
        wire_count: 0,
    };

    for (uid, file) in doc.dependencies() {
        reader.dependencies.insert(uid.to_string(), file.to_string());
    }

    // Load resources before resolving graph references
    let content = doc.content();
    let mut pkg = Package::new(&package_name);
    pkg.format_version = doc.format_version().filter(|version| !version.trim().is_empty()).map(str::to_string);
    let mut graph_elements: Vec<(String, XmlNode)> = Vec::new();
    if let Some(content) = content {
        reader.collect_items(content, "", &mut pkg, &mut graph_elements, linked);
        super::linked_resources::read(&mut reader, content, &mut pkg, linked);
        super::resource_replacement::read_direct(&mut reader, content, &mut pkg, linked);
    }

    // Name every graph first so instances can refer to graphs defined later in the file
    let mut names: Vec<String> = Vec::new();
    for (path, graph) in graph_elements.iter().take(MAX_GRAPHS) {
        let identifier = child_value(*graph, "identifier").unwrap_or("").trim();
        let name = if identifier.is_empty() { "Graph" } else { identifier }.to_string();
        reader.graph_names.insert(path.clone(), path.clone());
        names.push(name);
    }

    if graph_elements.len() > MAX_GRAPHS {
        reader.warnings.add(format!("only the first {MAX_GRAPHS} graphs were read"));
    }

    for ((path, graph), name) in graph_elements.iter().zip(names) {
        let mut graph = reader.read_graph(*graph, name);
        graph.folder = path.rsplit_once('/').map_or("", |(folder, _)| folder).into();
        pkg.graphs.push(graph);
    }

    report_placeholders(&pkg, reader.warnings);
    Ok(SbsRead { pkg, warnings: warnings.finish() })
}

pub fn is_sbs(bytes: &[u8]) -> bool {
    craft_sbs::is_sbs(bytes)
}
