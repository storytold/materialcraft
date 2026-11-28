use super::pattern_stamp::PatternStamp;
use super::stamp_cache::{self, Key, Properties};
use crate::function::{EvaluationState, PatternExpression, Program, flush_subnormal};
use materialcraft_core::{Value, ValueType};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// A scalar tint whose two branches use the same seeded value. One branch
/// multiplies it by a live variable. Its prefix also publishes the frame offset.
pub(super) struct ConditionalTint<'a> {
    pub offset: &'a str,
    condition: &'a str,
    threshold: f32,
    multiplier: &'a str,
    base: usize,
}

impl<'a> ConditionalTint<'a> {
    pub fn identify(program: &'a Program, assigned: &BTreeSet<&str>) -> Option<Self> {
        let inputs = |index: usize, name: &str| {
            let op = program.instructions.get(index)?;
            (op.definition.id == name).then_some(op.inputs.as_slice())
        };
        let variable = |index: usize, kind: &str| {
            let op = program.instructions.get(index)?;
            match &op.value {
                Some(Value::Text(name)) if op.definition.id == kind => Some(name.as_str()),
                _ => None,
            }
        };
        let &[prefix, result] = inputs(program.output, "sequence")? else { return None };
        let &[written] = inputs(prefix, "set")? else { return None };
        let offset = variable(prefix, "set")?;
        let &[test, scaled, base] = inputs(result, "ifelse")? else { return None };
        let &[a, b] = inputs(scaled, "mul")? else { return None };
        if program.instructions.get(scaled)?.type_code != ValueType::Float as u32 {
            return None;
        }
        let multiplier = variable(
            if a == base {
                b
            } else if b == base {
                a
            } else {
                return None;
            },
            "get_float1",
        )?;
        let &[variable_index, literal] = inputs(test, "gt")? else { return None };
        let condition = variable(variable_index, "get_float1")?;
        inputs(literal, "const_float1")?;
        let Some(Value::Float(threshold)) = program.instructions.get(literal)?.value.as_ref() else { return None };
        if !threshold.is_finite()
            || offset == condition
            || offset == multiplier
            || multiplier.starts_with('$')
            || (condition.starts_with('$') && condition != "$number")
        {
            return None;
        }
        // These two results can share operations and random draws. Capture the
        // ordinary evaluator's registers after it has followed the chosen branch.
        if !stamp_cache::reusable_inputs(program, &[written, base], assigned) {
            return None;
        }
        Some(Self { offset, condition, threshold: *threshold, multiplier, base })
    }

    pub fn scale(&self, variables: &BTreeMap<String, Value>) -> Option<TintScale> {
        let Value::Float(condition) = variables.get(self.condition)? else { return None };
        if !condition.is_finite() {
            return None;
        }
        if *condition > self.threshold {
            let Value::Float(scale) = variables.get(self.multiplier)? else { return None };
            scale.is_finite().then_some(TintScale::Multiply(*scale))
        } else {
            Some(TintScale::Identity)
        }
    }

    pub fn base(&self, state: &EvaluationState) -> Option<f32> {
        match state.registers.get(self.base)?.as_ref()? {
            Value::Float(value) if value.is_finite() => Some(*value),
            _ => None,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum TintScale {
    Identity,
    Multiply(f32),
}

impl TintScale {
    pub(super) fn apply(self, value: f32) -> f32 {
        match self {
            Self::Identity => value,
            Self::Multiply(scale) => flush_subnormal(flush_subnormal(value) * flush_subnormal(scale)),
        }
    }
}

pub(super) struct DrawingFields<'a> {
    pub fields: BTreeMap<&'static str, PatternExpression<'a>>,
    pub cost: usize,
}

#[derive(Default)]
pub(super) struct DrawingExpressions<'a>(BTreeMap<(u32, u32, bool), Option<DrawingFields<'a>>>);

impl<'a> DrawingExpressions<'a> {
    // Charge compact arithmetic in fractions of an ordinary property pass.
    // Even a completely folded draw still pays for property dispatch and placement.
    pub(super) const FULL_COST: usize = 128;

    pub(super) fn for_draw(
        &mut self,
        visit: &super::pattern_visit::PatternVisit,
        tint: Option<(&ConditionalTint<'_>, TintScale)>,
        placement: bool,
        programs: &'a super::evaluation::PropertyPrograms<'_>,
        variables: &BTreeMap<String, Value>,
        assigned: &BTreeSet<&str>,
    ) -> Option<&mut DrawingFields<'a>> {
        let scaled = matches!(tint, Some((_, TintScale::Multiply(_))));
        let key = (visit.node, visit.depth, scaled);
        // A graph can share a quadrant at many depths. Cap retained programs and failed bindings together.
        if self.0.len() >= 64 && !self.0.contains_key(&key) {
            return None;
        }
        self.0
            .entry(key)
            .or_insert_with(|| {
                let mut fields = BTreeMap::new();
                let (mut compact, mut ordinary) = (16, 16);
                let active: &[&str] = if tint.is_some() {
                    &["opacity", "branchoffset", "patternsize", "patternrotation"]
                } else if placement {
                    // The stamp cache verified that only these seeded fields vary for this quadrant.
                    &["opacity", "frameoffset", "patternrotation"]
                } else {
                    &["branchoffset"]
                };
                for &field in active {
                    if let Some((program, _)) = programs.get(&(visit.node, field)) {
                        let tint = tint.filter(|_| field == "opacity").map(|(tint, _)| (tint.base, scaled));
                        let expression = PatternExpression::bind(program, variables, assigned, tint)?;
                        let (steps, visits) = expression.work();
                        compact += steps;
                        ordinary += visits;
                        fields.insert(field, expression);
                    }
                }
                let cost = (compact * Self::FULL_COST).div_ceil(ordinary).clamp(Self::FULL_COST / 4, Self::FULL_COST);
                Some(DrawingFields { fields, cost })
            })
            .as_mut()
    }
}

#[derive(Default)]
pub(super) struct ConditionalStamps {
    fields: HashMap<(Key, bool), [f32; 8]>,
    templates: HashMap<(u32, u32), PatternStamp>,
}

impl ConditionalStamps {
    // The number of seeds can exceed the number of drawn pixels by several orders of magnitude.
    // Limit retained property sets for the whole traversal, including distinct branch outcomes.
    const MAX_ENTRIES: usize = 1_700_000;

    pub fn get(&self, key: Key, scale: TintScale) -> Option<Properties> {
        let &[bx, by, x, y, width, height, rotation, base] = self.fields.get(&(key, matches!(scale, TintScale::Multiply(_))))?;
        let alpha = scale.apply(base);
        if !alpha.is_finite() {
            return None;
        }
        let mut stamp = self.templates.get(&(key.node, key.depth))?.clone();
        stamp.center = [x, y];
        stamp.size = [width, height];
        stamp.rotation = rotation;
        stamp.color[3] = alpha;
        Some(Properties { branch_offset: [bx, by], stamp })
    }

    pub fn insert(&mut self, key: Key, scale: TintScale, base: f32, [bx, by]: [f32; 2], stamp: &PatternStamp) {
        if self.fields.len() >= Self::MAX_ENTRIES || scale.apply(base).to_bits() != stamp.color[3].to_bits() {
            return;
        }
        let [x, y] = stamp.center;
        let [width, height] = stamp.size;
        self.templates.entry((key.node, key.depth)).or_insert_with(|| stamp.clone());
        self.fields.insert((key, matches!(scale, TintScale::Multiply(_))), [bx, by, x, y, width, height, stamp.rotation, base]);
    }
}
