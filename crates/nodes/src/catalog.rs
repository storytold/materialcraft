use crate::NodeDefinition;
use std::sync::OnceLock;

/// Every node definition, in library order
pub fn catalog() -> &'static [NodeDefinition] {
    static CAT: OnceLock<Vec<NodeDefinition>> = OnceLock::new();
    CAT.get_or_init(|| {
        let mut defs = crate::atomic::defs();
        defs.extend(crate::adjustments::defs());
        defs.extend(crate::blending::defs());
        defs.extend(crate::blurs::defs());
        defs.extend(crate::flood_fill::defs());
        defs.extend(crate::material_filters::defs());
        defs.extend(crate::morphology::defs());
        defs.extend(crate::noises::defs());
        defs.extend(crate::normals::defs());
        defs.extend(crate::patterns::defs());
        defs.extend(crate::transforms::defs());
        defs.extend(crate::warps::defs());
        defs.extend(crate::weathering::defs());
        defs
    })
}

pub fn find(id: &str) -> Option<&'static NodeDefinition> {
    catalog().iter().find(|d| d.id == id)
}

/// The library's category tree, in display order: (path, ids)
pub fn categories() -> Vec<(&'static str, Vec<&'static str>)> {
    let mut out: Vec<(&'static str, Vec<&'static str>)> = Vec::new();
    for d in catalog() {
        match out.iter_mut().find(|(c, _)| *c == d.category) {
            Some((_, ids)) => ids.push(d.id),
            None => out.push((d.category, vec![d.id])),
        }
    }
    out
}
