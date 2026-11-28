use crate::function::Program;
use materialcraft_core::{Value, ValueType};
use std::collections::BTreeMap;

pub(super) fn declarations(program: &Program, names: &mut BTreeMap<String, ValueType>) {
    for op in &program.instructions {
        if op.definition.id == "set"
            && let Some(Value::Text(name)) = &op.value
            && let Some(kind) = ValueType::from_code(op.type_code)
        {
            names.insert(name.clone(), kind);
        }
        if let Some(call) = &op.call
            && call.exports
        {
            declarations(&call.program, names);
        }
    }
}

#[derive(Default)]
struct LocalValues {
    depth: Option<u32>,
    saved: BTreeMap<u32, Value>,
}

pub(super) struct Declaration {
    name: String,
    outer: Option<(u32, Value)>,
}

pub(super) struct PatternVariables<'a> {
    published: &'a BTreeMap<String, Value>,
    pub(super) drawing: BTreeMap<String, Value>,
    pub(super) modes: BTreeMap<String, Value>,
    pub(super) bound_draw: bool,
    locals: BTreeMap<String, LocalValues>,
    scopes: Vec<(usize, Vec<Declaration>)>,
}

impl<'a> PatternVariables<'a> {
    pub(super) fn new(published: &'a BTreeMap<String, Value>) -> Self {
        let mut drawing = published.clone();
        drawing.remove("$randomseed");
        let mut modes = drawing.clone();
        // Quadrant modes resolve before the traversal provides its position, depth and iteration.
        for name in ["$depth", "$depthpow2", "$number"] {
            modes.insert(name.into(), Value::Float(0.0));
        }
        Self { modes, drawing, published, bound_draw: false, locals: BTreeMap::new(), scopes: Vec::new() }
    }

    pub(super) fn seed_integers<'b>(&'b self, assigned: &'b [String]) -> impl Iterator<Item = (&'b str, i32)> {
        let locals = self.locals.iter().filter_map(|(name, local)| local.depth.map(|_| name));
        assigned.iter().chain(locals).filter_map(|name| match self.drawing.get(name) {
            Some(Value::Int(value)) => Some((name.as_str(), *value)),
            _ => None,
        })
    }

    pub(super) fn enter(&mut self, names: &BTreeMap<String, ValueType>, depth: u32, declared: &mut Vec<Declaration>) {
        for (name, kind) in names {
            let local = self.locals.entry(name.clone()).or_default();
            if local.depth == Some(depth) || (local.depth.is_none() && !self.modes.contains_key(name) && self.drawing.contains_key(name)) {
                continue;
            }
            // A child may read its parent's value. Declaring the name opens the
            // last value written at this quadrant depth, including across siblings.
            self.modes.remove(name);
            let value = local.saved.remove(&depth).unwrap_or_else(|| match (self.bound_draw, kind) {
                // A draw driven by a graph input starts these components undefined until the first write.
                (true, ValueType::Float) => Value::Float(f32::NAN),
                (true, ValueType::Float2) => Value::Float2([f32::NAN; 2]),
                (true, ValueType::Float3) => Value::Float3([f32::NAN; 3]),
                _ => kind.default_value(),
            });
            let previous = self.drawing.insert(name.clone(), value);
            let outer = local.depth.replace(depth).zip(previous);
            declared.push(Declaration { name: name.clone(), outer });
        }
    }

    pub(super) fn keep_for_children(&mut self, pending: usize, declared: Vec<Declaration>) {
        if !declared.is_empty() {
            self.scopes.push((pending, declared));
        }
    }

    pub(super) fn start_root_child(&mut self) {
        // A six-level quadrant tree starts a separate evaluation task per root child.
        // Active declarations on the root still supply values to read-only descendants.
        for local in self.locals.values_mut() {
            local.saved.clear();
        }
    }

    pub(super) fn resume(&mut self, pending_after_pop: usize) {
        // Once a waiting sibling is popped, the declaring node and all its descendants have finished.
        while self.scopes.last().is_some_and(|(pending, _)| pending_after_pop < *pending) {
            if let Some((_, declared)) = self.scopes.pop() {
                for Declaration { name, outer } in declared {
                    let Some(local) = self.locals.get_mut(&name) else { continue };
                    if let (Some(depth), Some(value)) = (local.depth.take(), self.drawing.remove(&name)) {
                        local.saved.insert(depth, value);
                    }
                    if let Some((depth, value)) = outer {
                        local.depth = Some(depth);
                        self.drawing.insert(name, value);
                    } else if let Some(value) = self.published.get(&name) {
                        self.drawing.insert(name.clone(), value.clone());
                        self.modes.insert(name, value.clone());
                    }
                }
            }
        }
    }
}
