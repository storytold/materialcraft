//! `session.inspect`: everything an agent needs to verify its work without a screenshot

use serde_json::{Value as J, json};

use crate::Session;

pub fn session(session: &Session, requested_graph: Option<usize>) -> J {
    let graph_index = requested_graph.unwrap_or(session.graph);
    let graph = session.pkg.graphs.get(graph_index);
    let cooked = (graph_index == session.graph).then_some(&session.cooked);
    let graph_json = graph.map(|graph| {
        let mut result = json!({
            "index": graph_index,
            "name": graph.name,
            "path": graph.sbs_path(),
            "kind": graph.kind(),
            "function": graph.function,
            "functionWarnings": crate::function_inspection::warnings(graph).into_iter().map(|(target, messages)| json!({"target": target, "messages": messages})).collect::<Vec<_>>(),
            "size": graph.size_px(),
            "format": graph.format,
            "seed": graph.seed,
            "attributes": graph.attributes,
            "exposed": graph.exposed,
            "presets": graph.presets,
            "inputs": graph.inputs,
            "inputImages": graph.input_images,
            "inputImagesLinear": graph.input_images_linear,
            "nodes": graph.nodes.iter().map(|node| {
                let entry = cooked.and_then(|cooked| cooked.nodes.get(&node.id));
                let (inputs, outputs) = materialcraft_eval::ports(&session.pkg, graph, node);
                let node_cooked = entry.map(|entry| {
                    let first_img = entry.outputs.first().and_then(materialcraft_eval::NodeOutput::as_image);
                    let stats = (node.def == "output").then_some(first_img).flatten().and_then(image_stats);
                    let mut obj = json!({
                        "error": entry.error, "ms": entry.ms, "cached": entry.cached,
                        "base": entry.base, "baseValues": entry.base_values, "parameters": entry.parameters,
                        "value": entry.outputs.first().and_then(materialcraft_eval::NodeOutput::as_value),
                        "size": first_img.map(|image| [image.width(), image.height()]),
                        "kind": first_img.map(|image| if image.is_gray() { "gray" } else { "color" }),
                        "mean": first_img.map(|image| image.mean()),
                    });
                    if let Some(stats) = stats {
                        obj["stats"] = stats;
                    }
                    obj
                });
                json!({
                    "id": node.id.0,
                    "def": node.def,
                    "instance": node.instance,
                    "label": if node.label.is_empty() { materialcraft_nodes::find(&node.def).map_or(node.def.as_str(), |definition| definition.label).to_string() } else { node.label.clone() },
                    "pos": node.pos,
                    "params": node.params,
                    "bindings": node.bindings,
                    "functions": node.functions,
                    "inputValues": node.input_values,
                    "base": node.base,
                    "disabled": node.disabled,
                    "inputs": inputs.iter().map(|port| &port.id).collect::<Vec<_>>(),
                    "outputs": outputs.iter().map(|port| &port.id).collect::<Vec<_>>(),
                    "cooked": node_cooked,
                })
            }).collect::<Vec<_>>(),
            "connections": graph.connections,
            "frames": graph.frames,
            "comments": graph.comments,
            "pins": graph.pins,
        });
        result["physicalSize"] = json!(graph.physical_size);
        result["pixelRatio"] = json!(graph.pixel_ratio);
        result["pixelRatioMode"] = json!(graph.pixel_ratio_mode);
        result["tiling"] = json!(graph.tiling);
        result["tilingMode"] = json!(graph.tiling_mode);
        result
    });

    json!({
        "documents": session.inspect_documents(),
        "package": session.has_package().then(|| json!({"name": session.pkg.name, "path": session.path, "modified": session.modified, "folders": session.pkg.folder_paths(), "folderDescriptions": session.pkg.folder_descriptions, "warnings": session.load_warnings, "resources": session.pkg.resources.iter().map(|entry| json!({"name": entry.name, "folder": entry.folder, "file": entry.file, "missing": entry.bytes.is_empty(), "bytes": entry.bytes.len()})).collect::<Vec<_>>()})),
        "graphs": session.pkg.graphs.iter().enumerate().map(|(index, graph)| json!({"index": index, "name": graph.name, "path": graph.sbs_path(), "nodes": graph.nodes.len(), "kind": graph.kind()})).collect::<Vec<_>>(),
        "openGraph": graph.map(|_| session.graph),
        "graph": graph_json,
        "annotationSelection": session.annotation_selection,
        "annotationTarget": session.annotation_target,
        "selection": session.selection.iter().map(|index| index.0).collect::<Vec<_>>(),
        "view2d": session.view2d,
        "view3d": session.view3d,
        "material3d": {
            "assignments": session.material_preview,
            "channels": session.material_preview.sources(&session.pkg).into_iter().map(|(channel, source)| {
                let image = session.material_image(&source);
                json!({"channel": channel, "source": source, "size": image.map(|image| [image.width(), image.height()]), "mean": image.map(|image| image.mean())})
            }).collect::<Vec<_>>(),
        },
        "previewSize": session.preview_size,
        "camera": session.camera,
        "environment": session.environment,
        "cookMs": session.cooked.ms,
        "dirty": session.dirty,
        "history": session.history(),
        "historyPreferences": session.history_prefs,
        "libraryCachePreferences": session.library_cache_prefs,
        "canUndo": session.can_undo(),
        "canRedo": session.can_redo(),
        "log": session.log.iter().rev().take(20).collect::<Vec<_>>(),
    })
}

/// Fast PBR range, contrast, and tile-seam diagnostics for output nodes (runs on preview images).
fn image_stats(image: &craft_texture::Image) -> Option<J> {
    let w = image.width() as usize;
    let h = image.height() as usize;
    let n = w.checked_mul(h)?;
    if n == 0 {
        return None;
    }
    let ch = if image.is_gray() { 1 } else { 4 };
    let data = image.data();
    let mut vals = Vec::with_capacity(n);
    let mut sum = 0.0f64;
    let mut sum_sq = 0.0f64;
    let mut min_v = f32::INFINITY;
    let mut max_v = f32::NEG_INFINITY;
    for px in data.chunks_exact(ch) {
        let v = if ch == 1 { px[0] } else { 0.2126 * px[0] + 0.7152 * px[1] + 0.0722 * px[2] };
        if !v.is_finite() {
            continue;
        }
        vals.push(v);
        let d = f64::from(v);
        sum += d;
        sum_sq += d * d;
        if v < min_v {
            min_v = v;
        }
        if v > max_v {
            max_v = v;
        }
    }
    if vals.is_empty() {
        return None;
    }
    let count = vals.len() as f64;
    let mean = sum / count;
    let var = (sum_sq / count - mean * mean).max(0.0);
    let std = var.sqrt();
    let len = vals.len();
    let p5_idx = (len * 5) / 100;
    let p95_idx = ((len * 95) / 100).min(len - 1);
    let (_, p5_ref, _) = vals.select_nth_unstable_by(p5_idx, f32::total_cmp);
    let p5 = *p5_ref;
    let (_, p95_ref, _) = vals.select_nth_unstable_by(p95_idx, f32::total_cmp);
    let p95 = *p95_ref;

    // Tile-wrap seam ratio: mean abs diff across border wrap divided by interior neighbor diff.
    let (seam_x, seam_y) = if w >= 4 && h >= 4 && data.len() == n * ch {
        let px_diff = |a: usize, b: usize| -> f64 {
            let ia = a * ch;
            let ib = b * ch;
            if ch == 1 {
                f64::from((data[ia] - data[ib]).abs())
            } else {
                (f64::from((data[ia] - data[ib]).abs()) + f64::from((data[ia + 1] - data[ib + 1]).abs()) + f64::from((data[ia + 2] - data[ib + 2]).abs())) / 3.0
            }
        };
        let mut bx = 0.0;
        let mut ix = 0.0;
        for y in 0..h {
            let row = y * w;
            bx += px_diff(row, row + w - 1);
            ix += px_diff(row + w / 2 - 1, row + w / 2);
        }
        let mut by = 0.0;
        let mut iy = 0.0;
        let last_row = (h - 1) * w;
        let mid_a = (h / 2 - 1) * w;
        let mid_b = (h / 2) * w;
        for x in 0..w {
            by += px_diff(x, last_row + x);
            iy += px_diff(mid_a + x, mid_b + x);
        }
        let seam_ratio = |border: f64, interior: f64| -> Option<f64> {
            if interior > 1e-6 {
                Some(border / interior)
            } else if border <= 1e-6 {
                Some(1.0)
            } else {
                None
            }
        };
        (seam_ratio(bx, ix), seam_ratio(by, iy))
    } else {
        (None, None)
    };

    Some(json!({
        "min": min_v,
        "p5": p5,
        "mean": mean,
        "p95": p95,
        "max": max_v,
        "std": std,
        "seamX": seam_x,
        "seamY": seam_y,
    }))
}

#[cfg(test)]
mod tests {
    use super::image_stats;
    use craft_texture::{Image, Kind, Limits};

    #[test]
    fn seam_ratio_distinguishes_constant_image_from_flat_interior_edge_discontinuity() {
        let flat = Image::from_data(4, 4, Kind::Gray, vec![0.5; 16], &Limits::default()).unwrap();
        let flat_stats = image_stats(&flat).unwrap();
        assert_eq!(flat_stats["seamX"].as_f64(), Some(1.0));
        assert_eq!(flat_stats["seamY"].as_f64(), Some(1.0));

        // Each row is [0, 0, 0, 1]: interior diff ix == 0, border wrap diff bx > 0.
        let mut step_data = Vec::with_capacity(16);
        for _ in 0..4 {
            step_data.extend_from_slice(&[0.0, 0.0, 0.0, 1.0]);
        }
        let step = Image::from_data(4, 4, Kind::Gray, step_data, &Limits::default()).unwrap();
        let step_stats = image_stats(&step).unwrap();
        assert!(step_stats["seamX"].is_null(), "edge discontinuity with flat interior must not report 1.0: {step_stats}");
        assert_eq!(step_stats["seamY"].as_f64(), Some(1.0));
    }
}
