//! Scalarized 8-wide SOA bytecode evaluator for pure `pixel_processor` function graphs.
//!
//! Pure DAG function graphs (no `while` loops, no nested graph calls, no stateful `rand`, and no
//! conditional `set`) are compiled once per cook into scalar `[f32; 8]` / `[bool; 8]` / `[i32; 8]`
//! operations over 8 horizontal pixels at a time:
//! - `set`/`get`/`sequence`/`passthrough`/`swizzle`/`vector` are resolved purely at compile time
//!   (zero runtime ops).
//! - Constants and pure constant subexpressions are folded once into pre-splatted register slots
//!   (zero runtime ops).
//! - `x - floor(x)` is fused into `FractF` and unreachable ops are eliminated by DCE.
//! - Each runtime op loops over fixed 8-wide `[f32; 8]` lanes so LLVM auto-vectorizes into SIMD
//!   registers with `#![forbid(unsafe_code)]` and full `wasm32` compatibility while staying
//!   bit-identical to `Program::evaluate_pixel`.

use std::collections::BTreeMap;

use super::{
    EvaluationContext, FunctionScope, Program, SystemVariable,
    instruction::Instruction,
    value_type::{dimensions, floats, integers},
};
use materialcraft_core::Value;

pub(crate) const PACKET: usize = 8;

#[derive(Clone, Copy, PartialEq, Eq)]
enum RegKind {
    Float,
    Int,
    Bool,
}

#[derive(Clone, Copy)]
struct RegRef {
    kind: RegKind,
    dims: u8,
    slots: [u16; 4],
}

#[derive(Clone, Copy)]
enum PacketOp {
    // Float unary (1 scalar [f32; PACKET] slot)
    NegF { dst: u16, a: u16 },
    AbsF { dst: u16, a: u16 },
    FloorF { dst: u16, a: u16 },
    FractF { dst: u16, a: u16 },
    CeilF { dst: u16, a: u16 },
    SqrtF { dst: u16, a: u16 },
    ExpF { dst: u16, a: u16 },
    LogF { dst: u16, a: u16 },
    Log2F { dst: u16, a: u16 },
    Pow2F { dst: u16, a: u16 },
    SinF { dst: u16, a: u16 },
    CosF { dst: u16, a: u16 },
    TanF { dst: u16, a: u16 },
    // Float binary
    AddF { dst: u16, a: u16, b: u16 },
    SubF { dst: u16, a: u16, b: u16 },
    MulF { dst: u16, a: u16, b: u16 },
    DivF { dst: u16, a: u16, b: u16 },
    ModF { dst: u16, a: u16, b: u16 },
    MinF { dst: u16, a: u16, b: u16 },
    MaxF { dst: u16, a: u16, b: u16 },
    PowF { dst: u16, a: u16, b: u16 },
    Atan2F { dst: u16, y: u16, x: u16 },
    CartesianF { dst_x: u16, dst_y: u16, r: u16, ang: u16 },
    // Float ternary
    LerpF { dst: u16, a: u16, b: u16, t: u16 },
    // Comparisons & logic (bool slot)
    EqF { dst: u16, a: u16, b: u16 },
    EqI { dst: u16, a: u16, b: u16 },
    EqB { dst: u16, a: u16, b: u16 },
    GtF { dst: u16, a: u16, b: u16 },
    GtEqF { dst: u16, a: u16, b: u16 },
    LrF { dst: u16, a: u16, b: u16 },
    LrEqF { dst: u16, a: u16, b: u16 },
    GtI { dst: u16, a: u16, b: u16 },
    GtEqI { dst: u16, a: u16, b: u16 },
    LrI { dst: u16, a: u16, b: u16 },
    LrEqI { dst: u16, a: u16, b: u16 },
    MinI { dst: u16, a: u16, b: u16 },
    MaxI { dst: u16, a: u16, b: u16 },
    IntToFloat { dst: u16, a: u16 },
    And { dst: u16, a: u16, b: u16 },
    Or { dst: u16, a: u16, b: u16 },
    Not { dst: u16, a: u16 },
    // Conditional select
    SelectF { dst: u16, c: u16, t: u16, e: u16 },
    SelectI { dst: u16, c: u16, t: u16, e: u16 },
    SelectB { dst: u16, c: u16, t: u16, e: u16 },
    // Texture sampling
    SampleLum { dst: u16, u: u16, v: u16, index: u16, bilinear: bool },
    SampleCol { dst: [u16; 4], u: u16, v: u16, index: u16, bilinear: bool },
}

pub(crate) struct FastRegs {
    f: Vec<[f32; PACKET]>,
    i: Vec<[i32; PACKET]>,
    b: Vec<[bool; PACKET]>,
}

pub(crate) struct PacketCtx<'a> {
    pub(crate) images: &'a [Option<craft_texture::Image>],
    pub(crate) tile: bool,
    pub(crate) gray: bool,
    pub(crate) format: materialcraft_core::OutputFormat,
    pub(crate) channels: usize,
}

pub(crate) struct FastProgram {
    ops: Vec<PacketOp>,
    f_init: Vec<f32>,
    i_init: Vec<i32>,
    b_init: Vec<bool>,
    pos_x: u16,
    pos_y: u16,
    output: RegRef,
}

struct Builder {
    ops: Vec<PacketOp>,
    f_init: Vec<f32>,
    f_const: Vec<Option<f32>>,
    f_const_pool: BTreeMap<u32, u16>,
    i_init: Vec<i32>,
    i_const: Vec<Option<i32>>,
    b_init: Vec<bool>,
    b_const: Vec<Option<bool>>,
    floor_of: BTreeMap<u16, u16>,
}

impl Builder {
    fn new() -> Self {
        Self {
            ops: Vec::new(),
            f_init: Vec::new(),
            f_const: Vec::new(),
            f_const_pool: BTreeMap::new(),
            i_init: Vec::new(),
            i_const: Vec::new(),
            b_init: Vec::new(),
            b_const: Vec::new(),
            floor_of: BTreeMap::new(),
        }
    }

    fn alloc_f_dyn(&mut self) -> Option<u16> {
        let id = u16::try_from(self.f_init.len()).ok()?;
        self.f_init.push(0.0);
        self.f_const.push(None);
        Some(id)
    }

    fn alloc_f_const(&mut self, val: f32) -> Option<u16> {
        let bits = val.to_bits();
        if let Some(&slot) = self.f_const_pool.get(&bits) {
            return Some(slot);
        }
        let id = u16::try_from(self.f_init.len()).ok()?;
        self.f_init.push(val);
        self.f_const.push(Some(val));
        self.f_const_pool.insert(bits, id);
        Some(id)
    }

    fn alloc_i_dyn(&mut self) -> Option<u16> {
        let id = u16::try_from(self.i_init.len()).ok()?;
        self.i_init.push(0);
        self.i_const.push(None);
        Some(id)
    }

    fn alloc_i_const(&mut self, val: i32) -> Option<u16> {
        let id = u16::try_from(self.i_init.len()).ok()?;
        self.i_init.push(val);
        self.i_const.push(Some(val));
        Some(id)
    }

    fn alloc_b_dyn(&mut self) -> Option<u16> {
        let id = u16::try_from(self.b_init.len()).ok()?;
        self.b_init.push(false);
        self.b_const.push(None);
        Some(id)
    }

    fn alloc_b_const(&mut self, val: bool) -> Option<u16> {
        let id = u16::try_from(self.b_init.len()).ok()?;
        self.b_init.push(val);
        self.b_const.push(Some(val));
        Some(id)
    }

    fn f_c(&self, slot: u16) -> Option<f32> {
        self.f_const.get(usize::from(slot)).copied().flatten()
    }

    fn i_c(&self, slot: u16) -> Option<i32> {
        self.i_const.get(usize::from(slot)).copied().flatten()
    }

    fn b_c(&self, slot: u16) -> Option<bool> {
        self.b_const.get(usize::from(slot)).copied().flatten()
    }

    fn const_value(&mut self, value: &Value) -> Option<RegRef> {
        match value {
            Value::Bool(b) => Some(RegRef { kind: RegKind::Bool, dims: 1, slots: [self.alloc_b_const(*b)?, 0, 0, 0] }),
            Value::Int(x) => Some(RegRef { kind: RegKind::Int, dims: 1, slots: [self.alloc_i_const(*x)?, 0, 0, 0] }),
            Value::Int2(v) => Some(RegRef { kind: RegKind::Int, dims: 2, slots: [self.alloc_i_const(v[0])?, self.alloc_i_const(v[1])?, 0, 0] }),
            Value::Int3(v) => {
                Some(RegRef { kind: RegKind::Int, dims: 3, slots: [self.alloc_i_const(v[0])?, self.alloc_i_const(v[1])?, self.alloc_i_const(v[2])?, 0] })
            }
            Value::Int4(v) => Some(RegRef {
                kind: RegKind::Int,
                dims: 4,
                slots: [self.alloc_i_const(v[0])?, self.alloc_i_const(v[1])?, self.alloc_i_const(v[2])?, self.alloc_i_const(v[3])?],
            }),
            Value::Enum(x) => Some(RegRef { kind: RegKind::Int, dims: 1, slots: [self.alloc_i_const(i32::try_from(*x).ok()?)?, 0, 0, 0] }),
            Value::Float(x) => Some(RegRef { kind: RegKind::Float, dims: 1, slots: [self.alloc_f_const(*x)?, 0, 0, 0] }),
            Value::Float2(v) => Some(RegRef { kind: RegKind::Float, dims: 2, slots: [self.alloc_f_const(v[0])?, self.alloc_f_const(v[1])?, 0, 0] }),
            Value::Float3(v) => {
                Some(RegRef { kind: RegKind::Float, dims: 3, slots: [self.alloc_f_const(v[0])?, self.alloc_f_const(v[1])?, self.alloc_f_const(v[2])?, 0] })
            }
            Value::Float4(v) | Value::Color(v) => Some(RegRef {
                kind: RegKind::Float,
                dims: 4,
                slots: [self.alloc_f_const(v[0])?, self.alloc_f_const(v[1])?, self.alloc_f_const(v[2])?, self.alloc_f_const(v[3])?],
            }),
            _ => None,
        }
    }

    fn emit_unary_f(&mut self, op: &str, a: u16) -> Option<u16> {
        if let Some(va) = self.f_c(a) {
            let r = match op {
                "neg" => -va,
                "abs" => va.abs(),
                "floor" => va.floor(),
                "ceil" => va.ceil(),
                "sqrt" => va.sqrt(),
                "exp" => va.exp(),
                "log" => va.ln(),
                "log2" => va.log2(),
                "pow2" => va.exp2(),
                "sin" => va.sin(),
                "cos" => va.cos(),
                _ => va.tan(),
            };
            return self.alloc_f_const(r);
        }
        let dst = self.alloc_f_dyn()?;
        let pkt = match op {
            "neg" => PacketOp::NegF { dst, a },
            "abs" => PacketOp::AbsF { dst, a },
            "floor" => {
                self.floor_of.insert(dst, a);
                PacketOp::FloorF { dst, a }
            }
            "ceil" => PacketOp::CeilF { dst, a },
            "sqrt" => PacketOp::SqrtF { dst, a },
            "exp" => PacketOp::ExpF { dst, a },
            "log" => PacketOp::LogF { dst, a },
            "log2" => PacketOp::Log2F { dst, a },
            "pow2" => PacketOp::Pow2F { dst, a },
            "sin" => PacketOp::SinF { dst, a },
            "cos" => PacketOp::CosF { dst, a },
            _ => PacketOp::TanF { dst, a },
        };
        self.ops.push(pkt);
        Some(dst)
    }

    fn emit_bin_f(&mut self, op: &str, a: u16, b: u16) -> Option<u16> {
        if let (Some(va), Some(vb)) = (self.f_c(a), self.f_c(b)) {
            let r = match op {
                "add" => va + vb,
                "sub" => va - vb,
                "mul" => va * vb,
                "div" => va / vb,
                "mod" => va - vb * (va / vb).floor(),
                "min" => va.min(vb),
                "max" => va.max(vb),
                _ => va.powf(vb),
            };
            return self.alloc_f_const(r);
        }
        if op == "sub" && self.floor_of.get(&b).copied() == Some(a) {
            let dst = self.alloc_f_dyn()?;
            self.ops.push(PacketOp::FractF { dst, a });
            return Some(dst);
        }
        let dst = self.alloc_f_dyn()?;
        let pkt = match op {
            "add" => PacketOp::AddF { dst, a, b },
            "sub" => PacketOp::SubF { dst, a, b },
            "mul" => PacketOp::MulF { dst, a, b },
            "div" => PacketOp::DivF { dst, a, b },
            "mod" => PacketOp::ModF { dst, a, b },
            "min" => PacketOp::MinF { dst, a, b },
            "max" => PacketOp::MaxF { dst, a, b },
            _ => PacketOp::PowF { dst, a, b },
        };
        self.ops.push(pkt);
        Some(dst)
    }

    fn emit_lerp_f(&mut self, a: u16, b: u16, t: u16) -> Option<u16> {
        if let (Some(va), Some(vb), Some(vt)) = (self.f_c(a), self.f_c(b), self.f_c(t)) {
            return self.alloc_f_const(va + (vb - va) * vt);
        }
        let dst = self.alloc_f_dyn()?;
        self.ops.push(PacketOp::LerpF { dst, a, b, t });
        Some(dst)
    }

    fn emit_and(&mut self, a: u16, b: u16) -> Option<u16> {
        if let (Some(va), Some(vb)) = (self.b_c(a), self.b_c(b)) {
            return self.alloc_b_const(va && vb);
        }
        let dst = self.alloc_b_dyn()?;
        self.ops.push(PacketOp::And { dst, a, b });
        Some(dst)
    }

    fn emit_or(&mut self, a: u16, b: u16) -> Option<u16> {
        if let (Some(va), Some(vb)) = (self.b_c(a), self.b_c(b)) {
            return self.alloc_b_const(va || vb);
        }
        let dst = self.alloc_b_dyn()?;
        self.ops.push(PacketOp::Or { dst, a, b });
        Some(dst)
    }

    fn emit_not(&mut self, a: u16) -> Option<u16> {
        if let Some(va) = self.b_c(a) {
            return self.alloc_b_const(!va);
        }
        let dst = self.alloc_b_dyn()?;
        self.ops.push(PacketOp::Not { dst, a });
        Some(dst)
    }
}

impl FastProgram {
    /// Compile `program` into a scalarized 8-wide SOA packet program if every instruction is a pure
    /// pixel-processor op with static types and no loops/calls/conditional side effects.
    pub(crate) fn try_compile(program: &Program, vars: &BTreeMap<String, Value>, size: [f32; 2], tile: bool) -> Option<Self> {
        let len = program.instructions.len();
        if len == 0 || len > usize::from(u16::MAX) {
            return None;
        }
        let conditional = conditional_instructions(&program.instructions);
        let mut set_slots: BTreeMap<&str, RegRef> = BTreeMap::new();
        let mut refs: Vec<RegRef> = Vec::with_capacity(len);
        let mut b = Builder::new();
        let pos_x = b.alloc_f_dyn()?;
        let pos_y = b.alloc_f_dyn()?;
        let pos_ref = RegRef { kind: RegKind::Float, dims: 2, slots: [pos_x, pos_y, 0, 0] };
        let zero_f = b.alloc_f_const(0.0)?;

        let dummy_ctx = EvaluationContext { scope: FunctionScope::PixelProcessor, position: [0.0, 0.0], size, images: &[], tile, variables: vars };

        for (idx, inst) in program.instructions.iter().enumerate() {
            if inst.call.is_some() {
                return None;
            }
            let op = inst.definition.id;
            let in_ref = |slot: usize| -> Option<RegRef> {
                let src = *inst.inputs.get(slot)?;
                refs.get(src).copied()
            };

            let out = match op {
                "passthrough" => in_ref(0)?,
                "sequence" => in_ref(1)?,
                "set" => {
                    if *conditional.get(idx).unwrap_or(&true) {
                        return None;
                    }
                    let Some(Value::Text(name)) = &inst.value else { return None };
                    if name.is_empty() || name.starts_with('$') || set_slots.contains_key(name.as_str()) {
                        return None;
                    }
                    let r = in_ref(0)?;
                    set_slots.insert(name.as_str(), r);
                    r
                }
                op if op.starts_with("const_") => {
                    let val = inst.value.as_ref()?;
                    if super::value_type::type_code(val) != inst.type_code {
                        return None;
                    }
                    let canonical = super::value_type::canonical(val.clone()).ok()?;
                    b.const_value(&canonical)?
                }
                op if op.starts_with("get_") => {
                    let Some(Value::Text(name)) = &inst.value else { return None };
                    if name == "$pos" {
                        if inst.type_code != 512 {
                            return None;
                        }
                        pos_ref
                    } else if let Some(&r) = set_slots.get(name.as_str()) {
                        if reg_type_code(r.kind, r.dims) != inst.type_code {
                            return None;
                        }
                        r
                    } else if SystemVariable::named(name).is_some() {
                        let resolved = dummy_ctx.variable(name, &BTreeMap::new()).ok()??;
                        if super::value_type::type_code(&resolved) != inst.type_code {
                            return None;
                        }
                        let canonical = super::value_type::canonical(resolved).ok()?;
                        b.const_value(&canonical)?
                    } else {
                        if program
                            .instructions
                            .get(idx + 1..)
                            .is_some_and(|rest| rest.iter().any(|r| r.definition.id == "set" && matches!(&r.value, Some(Value::Text(n)) if n == name)))
                        {
                            return None;
                        }
                        let val = match vars.get(name.as_str()).cloned() {
                            Some(v) => v,
                            None => {
                                if name.starts_with('$') {
                                    return None;
                                }
                                materialcraft_core::ValueType::from_code(inst.type_code)
                                    .filter(|k| *k != materialcraft_core::ValueType::String)?
                                    .default_value()
                            }
                        };
                        let val = if op == "get_bool" && !name.starts_with('$') && matches!(val, Value::Int(_) | Value::Enum(_)) {
                            Value::Bool(val.as_i32()? != 0)
                        } else {
                            val
                        };
                        if super::value_type::type_code(&val) != inst.type_code {
                            return None;
                        }
                        let canonical = super::value_type::canonical(val).ok()?;
                        b.const_value(&canonical)?
                    }
                }
                "neg" | "abs" | "floor" | "ceil" | "sqrt" | "exp" | "log" | "log2" | "pow2" | "sin" | "cos" | "tan" => {
                    let a = in_ref(0)?;
                    if a.kind != RegKind::Float {
                        return None;
                    }
                    let mut slots = [0u16; 4];
                    for k in 0..usize::from(a.dims) {
                        *slots.get_mut(k)? = b.emit_unary_f(op, *a.slots.get(k)?)?;
                    }
                    RegRef { kind: RegKind::Float, dims: a.dims, slots }
                }
                "add" | "sub" | "mul" | "div" | "mod" | "min" | "max" | "pow" => {
                    let ra = in_ref(0)?;
                    let rb = in_ref(1)?;
                    if ra.kind == RegKind::Int && rb.kind == RegKind::Int && ra.dims == rb.dims && matches!(op, "min" | "max") {
                        let mut slots = [0u16; 4];
                        for k in 0..usize::from(ra.dims) {
                            let sa = *ra.slots.get(k)?;
                            let sb = *rb.slots.get(k)?;
                            let d = if let (Some(va), Some(vb)) = (b.i_c(sa), b.i_c(sb)) {
                                b.alloc_i_const(if op == "min" { va.min(vb) } else { va.max(vb) })?
                            } else {
                                let d = b.alloc_i_dyn()?;
                                b.ops.push(if op == "min" { PacketOp::MinI { dst: d, a: sa, b: sb } } else { PacketOp::MaxI { dst: d, a: sa, b: sb } });
                                d
                            };
                            *slots.get_mut(k)? = d;
                        }
                        RegRef { kind: RegKind::Int, dims: ra.dims, slots }
                    } else {
                        if ra.kind != RegKind::Float || rb.kind != RegKind::Float || ra.dims != rb.dims {
                            return None;
                        }
                        let mut slots = [0u16; 4];
                        for k in 0..usize::from(ra.dims) {
                            *slots.get_mut(k)? = b.emit_bin_f(op, *ra.slots.get(k)?, *rb.slots.get(k)?)?;
                        }
                        RegRef { kind: RegKind::Float, dims: ra.dims, slots }
                    }
                }
                "tofloat" | "tofloat2" | "tofloat3" | "tofloat4" => {
                    let ra = in_ref(0)?;
                    let want = match op {
                        "tofloat" => 1u8,
                        "tofloat2" => 2,
                        "tofloat3" => 3,
                        _ => 4,
                    };
                    if ra.dims != want {
                        return None;
                    }
                    if ra.kind != RegKind::Int {
                        return None;
                    }
                    let mut slots = [0u16; 4];
                    for k in 0..usize::from(want) {
                        let sa = *ra.slots.get(k)?;
                        let d = if let Some(va) = b.i_c(sa) {
                            b.alloc_f_const(va as f32)?
                        } else {
                            let d = b.alloc_f_dyn()?;
                            b.ops.push(PacketOp::IntToFloat { dst: d, a: sa });
                            d
                        };
                        *slots.get_mut(k)? = d;
                    }
                    RegRef { kind: RegKind::Float, dims: want, slots }
                }
                "toint1" | "toint2" | "toint3" | "toint4" => {
                    let ra = in_ref(0)?;
                    let want = match op {
                        "toint1" => 1u8,
                        "toint2" => 2,
                        "toint3" => 3,
                        _ => 4,
                    };
                    if ra.dims != want || ra.kind != RegKind::Float {
                        return None;
                    }
                    // Only fold finite, in-range float constants using floor() (matching
                    // `vector::evaluate`). Decline dynamic or out-of-range casts so the
                    // interpreter retains exact range/NaN errors only on taken branches.
                    let mut slots = [0u16; 4];
                    for k in 0..usize::from(want) {
                        let sa = *ra.slots.get(k)?;
                        let va = b.f_c(sa)?;
                        if !va.is_finite() || !(-2_147_483_648.0f32..2_147_483_648.0f32).contains(&va) {
                            return None;
                        }
                        *slots.get_mut(k)? = b.alloc_i_const(va.floor() as i32)?;
                    }
                    RegRef { kind: RegKind::Int, dims: want, slots }
                }
                "mulscalar" => {
                    let ra = in_ref(0)?;
                    let rb = in_ref(1)?;
                    if ra.kind != RegKind::Float || rb.kind != RegKind::Float || rb.dims != 1 {
                        return None;
                    }
                    let s = rb.slots[0];
                    let mut slots = [0u16; 4];
                    for k in 0..usize::from(ra.dims) {
                        *slots.get_mut(k)? = b.emit_bin_f("mul", *ra.slots.get(k)?, s)?;
                    }
                    RegRef { kind: RegKind::Float, dims: ra.dims, slots }
                }
                "dot" => {
                    let ra = in_ref(0)?;
                    let rb = in_ref(1)?;
                    if ra.kind != RegKind::Float || rb.kind != RegKind::Float || ra.dims != rb.dims || ra.dims == 0 {
                        return None;
                    }
                    let mut acc = b.emit_bin_f("mul", ra.slots[0], rb.slots[0])?;
                    for k in 1..usize::from(ra.dims) {
                        let prod = b.emit_bin_f("mul", *ra.slots.get(k)?, *rb.slots.get(k)?)?;
                        acc = b.emit_bin_f("add", acc, prod)?;
                    }
                    RegRef { kind: RegKind::Float, dims: 1, slots: [acc, 0, 0, 0] }
                }
                "atan2" => {
                    let ra = in_ref(0)?;
                    if ra.kind != RegKind::Float || ra.dims != 2 {
                        return None;
                    }
                    let dst = if let (Some(vx), Some(vy)) = (b.f_c(ra.slots[0]), b.f_c(ra.slots[1])) {
                        b.alloc_f_const(vy.atan2(vx))?
                    } else {
                        let d = b.alloc_f_dyn()?;
                        b.ops.push(PacketOp::Atan2F { dst: d, y: ra.slots[1], x: ra.slots[0] });
                        d
                    };
                    RegRef { kind: RegKind::Float, dims: 1, slots: [dst, 0, 0, 0] }
                }
                "cartesian" => {
                    let ra = in_ref(0)?;
                    let rb = in_ref(1)?;
                    if ra.kind != RegKind::Float || rb.kind != RegKind::Float || ra.dims != 1 || rb.dims != 1 {
                        return None;
                    }
                    let (dx, dy) = if let (Some(vr), Some(va)) = (b.f_c(ra.slots[0]), b.f_c(rb.slots[0])) {
                        (b.alloc_f_const(vr * va.cos())?, b.alloc_f_const(vr * va.sin())?)
                    } else {
                        let dx = b.alloc_f_dyn()?;
                        let dy = b.alloc_f_dyn()?;
                        b.ops.push(PacketOp::CartesianF { dst_x: dx, dst_y: dy, r: ra.slots[0], ang: rb.slots[0] });
                        (dx, dy)
                    };
                    RegRef { kind: RegKind::Float, dims: 2, slots: [dx, dy, 0, 0] }
                }
                "lerp" => {
                    let ra = in_ref(0)?;
                    let rb = in_ref(1)?;
                    let rt = in_ref(2)?;
                    if ra.kind != RegKind::Float
                        || rb.kind != RegKind::Float
                        || rt.kind != RegKind::Float
                        || ra.dims != rb.dims
                        || (rt.dims != ra.dims && rt.dims != 1)
                    {
                        return None;
                    }
                    let mut slots = [0u16; 4];
                    for k in 0..usize::from(ra.dims) {
                        let ts = if rt.dims == 1 { rt.slots[0] } else { *rt.slots.get(k)? };
                        *slots.get_mut(k)? = b.emit_lerp_f(*ra.slots.get(k)?, *rb.slots.get(k)?, ts)?;
                    }
                    RegRef { kind: RegKind::Float, dims: ra.dims, slots }
                }
                "eq" | "noteq" => {
                    let ra = in_ref(0)?;
                    let rb = in_ref(1)?;
                    if ra.kind != rb.kind || ra.dims != rb.dims || ra.dims == 0 {
                        return None;
                    }
                    let mut eq_slot: Option<u16> = None;
                    for k in 0..usize::from(ra.dims) {
                        let sa = *ra.slots.get(k)?;
                        let sb = *rb.slots.get(k)?;
                        let cmp = match ra.kind {
                            RegKind::Float => {
                                if let (Some(va), Some(vb)) = (b.f_c(sa), b.f_c(sb)) {
                                    b.alloc_b_const(va == vb)?
                                } else {
                                    let d = b.alloc_b_dyn()?;
                                    b.ops.push(PacketOp::EqF { dst: d, a: sa, b: sb });
                                    d
                                }
                            }
                            RegKind::Int => {
                                if let (Some(va), Some(vb)) = (b.i_c(sa), b.i_c(sb)) {
                                    b.alloc_b_const(va == vb)?
                                } else {
                                    let d = b.alloc_b_dyn()?;
                                    b.ops.push(PacketOp::EqI { dst: d, a: sa, b: sb });
                                    d
                                }
                            }
                            RegKind::Bool => {
                                if let (Some(va), Some(vb)) = (b.b_c(sa), b.b_c(sb)) {
                                    b.alloc_b_const(va == vb)?
                                } else {
                                    let d = b.alloc_b_dyn()?;
                                    b.ops.push(PacketOp::EqB { dst: d, a: sa, b: sb });
                                    d
                                }
                            }
                        };
                        eq_slot = Some(match eq_slot {
                            None => cmp,
                            Some(prev) => b.emit_and(prev, cmp)?,
                        });
                    }
                    let res = eq_slot?;
                    let res = if op == "noteq" { b.emit_not(res)? } else { res };
                    RegRef { kind: RegKind::Bool, dims: 1, slots: [res, 0, 0, 0] }
                }
                "gt" | "gteq" | "lr" | "lreq" => {
                    let ra = in_ref(0)?;
                    let rb = in_ref(1)?;
                    if ra.kind != rb.kind || ra.dims != 1 || rb.dims != 1 {
                        return None;
                    }
                    let sa = ra.slots[0];
                    let sb = rb.slots[0];
                    let dst = match ra.kind {
                        RegKind::Float => {
                            if let (Some(va), Some(vb)) = (b.f_c(sa), b.f_c(sb)) {
                                let v = match op {
                                    "gt" => va > vb,
                                    "gteq" => va >= vb,
                                    "lr" => va < vb,
                                    _ => va <= vb,
                                };
                                b.alloc_b_const(v)?
                            } else {
                                let d = b.alloc_b_dyn()?;
                                let pkt = match op {
                                    "gt" => PacketOp::GtF { dst: d, a: sa, b: sb },
                                    "gteq" => PacketOp::GtEqF { dst: d, a: sa, b: sb },
                                    "lr" => PacketOp::LrF { dst: d, a: sa, b: sb },
                                    _ => PacketOp::LrEqF { dst: d, a: sa, b: sb },
                                };
                                b.ops.push(pkt);
                                d
                            }
                        }
                        RegKind::Int => {
                            if let (Some(va), Some(vb)) = (b.i_c(sa), b.i_c(sb)) {
                                let v = match op {
                                    "gt" => va > vb,
                                    "gteq" => va >= vb,
                                    "lr" => va < vb,
                                    _ => va <= vb,
                                };
                                b.alloc_b_const(v)?
                            } else {
                                let d = b.alloc_b_dyn()?;
                                let pkt = match op {
                                    "gt" => PacketOp::GtI { dst: d, a: sa, b: sb },
                                    "gteq" => PacketOp::GtEqI { dst: d, a: sa, b: sb },
                                    "lr" => PacketOp::LrI { dst: d, a: sa, b: sb },
                                    _ => PacketOp::LrEqI { dst: d, a: sa, b: sb },
                                };
                                b.ops.push(pkt);
                                d
                            }
                        }
                        RegKind::Bool => return None,
                    };
                    RegRef { kind: RegKind::Bool, dims: 1, slots: [dst, 0, 0, 0] }
                }
                "and" => {
                    let ra = in_ref(0)?;
                    let rb = in_ref(1)?;
                    if ra.kind != RegKind::Bool || rb.kind != RegKind::Bool {
                        return None;
                    }
                    let dst = b.emit_and(ra.slots[0], rb.slots[0])?;
                    RegRef { kind: RegKind::Bool, dims: 1, slots: [dst, 0, 0, 0] }
                }
                "or" => {
                    let ra = in_ref(0)?;
                    let rb = in_ref(1)?;
                    if ra.kind != RegKind::Bool || rb.kind != RegKind::Bool {
                        return None;
                    }
                    let dst = b.emit_or(ra.slots[0], rb.slots[0])?;
                    RegRef { kind: RegKind::Bool, dims: 1, slots: [dst, 0, 0, 0] }
                }
                "not" => {
                    let ra = in_ref(0)?;
                    if ra.kind != RegKind::Bool {
                        return None;
                    }
                    let dst = b.emit_not(ra.slots[0])?;
                    RegRef { kind: RegKind::Bool, dims: 1, slots: [dst, 0, 0, 0] }
                }
                "ifelse" => {
                    let rc = in_ref(0)?;
                    let rt = in_ref(1)?;
                    let re = in_ref(2)?;
                    if rc.kind != RegKind::Bool || rt.kind != re.kind || rt.dims != re.dims {
                        return None;
                    }
                    let c = rc.slots[0];
                    if let Some(cv) = b.b_c(c) {
                        if cv { rt } else { re }
                    } else {
                        let mut slots = [0u16; 4];
                        for k in 0..usize::from(rt.dims) {
                            let st = *rt.slots.get(k)?;
                            let se = *re.slots.get(k)?;
                            if st == se {
                                *slots.get_mut(k)? = st;
                            } else {
                                let d = match rt.kind {
                                    RegKind::Float => {
                                        let d = b.alloc_f_dyn()?;
                                        b.ops.push(PacketOp::SelectF { dst: d, c, t: st, e: se });
                                        d
                                    }
                                    RegKind::Int => {
                                        let d = b.alloc_i_dyn()?;
                                        b.ops.push(PacketOp::SelectI { dst: d, c, t: st, e: se });
                                        d
                                    }
                                    RegKind::Bool => {
                                        let d = b.alloc_b_dyn()?;
                                        b.ops.push(PacketOp::SelectB { dst: d, c, t: st, e: se });
                                        d
                                    }
                                };
                                *slots.get_mut(k)? = d;
                            }
                        }
                        RegRef { kind: rt.kind, dims: rt.dims, slots }
                    }
                }
                "swizzle1" | "swizzle2" | "swizzle3" | "swizzle4" => {
                    let ra = in_ref(0)?;
                    if ra.kind != RegKind::Float {
                        return None;
                    }
                    let count = u8::try_from(op.chars().last()?.to_digit(10)?).ok()?;
                    let raw = inst.value.as_ref().map(integers).transpose().ok()?.unwrap_or([0; 4]);
                    let mut slots = [0u16; 4];
                    for lane in 0..usize::from(count) {
                        let c = usize::try_from(*raw.get(lane)?).ok()?;
                        if c >= 4 {
                            return None;
                        }
                        *slots.get_mut(lane)? = if c < usize::from(ra.dims) { *ra.slots.get(c)? } else { zero_f };
                    }
                    RegRef { kind: RegKind::Float, dims: count, slots }
                }
                "vector2" | "vector3" | "vector4" => {
                    let ra = in_ref(0)?;
                    let rb = in_ref(1)?;
                    let count = u8::try_from(op.chars().last()?.to_digit(10)?).ok()?;
                    if ra.kind != RegKind::Float || rb.kind != RegKind::Float || ra.dims + rb.dims != count {
                        return None;
                    }
                    let mut slots = [0u16; 4];
                    let da = usize::from(ra.dims);
                    for k in 0..da {
                        *slots.get_mut(k)? = *ra.slots.get(k)?;
                    }
                    for k in 0..usize::from(rb.dims) {
                        *slots.get_mut(da + k)? = *rb.slots.get(k)?;
                    }
                    RegRef { kind: RegKind::Float, dims: count, slots }
                }
                "samplelum" | "samplecol" => {
                    let uv = in_ref(0)?;
                    if uv.kind != RegKind::Float || uv.dims != 2 {
                        return None;
                    }
                    let setting = inst.value.as_ref().map(integers).transpose().ok()?.unwrap_or([0; 4]);
                    let index = u16::try_from(setting[0]).ok()?;
                    let bilinear = setting[1] != 0;
                    if op == "samplelum" {
                        let d = b.alloc_f_dyn()?;
                        b.ops.push(PacketOp::SampleLum { dst: d, u: uv.slots[0], v: uv.slots[1], index, bilinear });
                        RegRef { kind: RegKind::Float, dims: 1, slots: [d, 0, 0, 0] }
                    } else {
                        let d0 = b.alloc_f_dyn()?;
                        let d1 = b.alloc_f_dyn()?;
                        let d2 = b.alloc_f_dyn()?;
                        let d3 = b.alloc_f_dyn()?;
                        b.ops.push(PacketOp::SampleCol { dst: [d0, d1, d2, d3], u: uv.slots[0], v: uv.slots[1], index, bilinear });
                        RegRef { kind: RegKind::Float, dims: 4, slots: [d0, d1, d2, d3] }
                    }
                }
                _ => return None,
            };

            refs.push(out);
        }

        let output = *refs.get(program.output)?;
        if output.kind != RegKind::Float {
            return None;
        }
        let expected_type = program.instructions.get(program.output)?.type_code;
        if reg_type_code(output.kind, output.dims) != expected_type {
            return None;
        }
        let _ = (dimensions, floats);

        let ops = eliminate_dead_ops(&b.ops, output, b.f_init.len(), b.i_init.len(), b.b_init.len());
        Some(Self { ops, f_init: b.f_init, i_init: b.i_init, b_init: b.b_init, pos_x, pos_y, output })
    }

    pub(crate) fn new_regs(&self) -> FastRegs {
        FastRegs {
            f: self.f_init.iter().map(|&v| [v; PACKET]).collect(),
            i: self.i_init.iter().map(|&v| [v; PACKET]).collect(),
            b: self.b_init.iter().map(|&v| [v; PACKET]).collect(),
        }
    }

    /// Evaluate 8 pixels simultaneously (`px[0..PACKET]` at row `py`) and write the first `count`
    /// pixels (`1..=PACKET`) into `target_row` (which must have length `count * ctx.channels`).
    #[inline]
    pub(crate) fn eval_packet(
        &self,
        regs: &mut FastRegs,
        px: [f32; PACKET],
        py: f32,
        ctx: &PacketCtx<'_>,
        count: usize,
        target_row: &mut [f32],
    ) -> Result<(), String> {
        let images = ctx.images;
        let tile = ctx.tile;
        let gray = ctx.gray;
        let format = ctx.format;
        let channels = ctx.channels;
        if let Some(slot) = regs.f.get_mut(usize::from(self.pos_x)) {
            *slot = px;
        }
        if let Some(slot) = regs.f.get_mut(usize::from(self.pos_y)) {
            *slot = [py; PACKET];
        }

        let rf = &mut regs.f;
        let ri = &mut regs.i;
        let rb = &mut regs.b;

        for &op in &self.ops {
            match op {
                PacketOp::NegF { dst, a } => {
                    let va = *rf.get(usize::from(a)).ok_or("invalid reg")?;
                    let mut out = [0.0f32; PACKET];
                    for k in 0..PACKET {
                        out[k] = -va[k];
                    }
                    *rf.get_mut(usize::from(dst)).ok_or("invalid reg")? = out;
                }
                PacketOp::AbsF { dst, a } => {
                    let va = *rf.get(usize::from(a)).ok_or("invalid reg")?;
                    let mut out = [0.0f32; PACKET];
                    for k in 0..PACKET {
                        out[k] = va[k].abs();
                    }
                    *rf.get_mut(usize::from(dst)).ok_or("invalid reg")? = out;
                }
                PacketOp::FloorF { dst, a } => {
                    let va = *rf.get(usize::from(a)).ok_or("invalid reg")?;
                    let mut out = [0.0f32; PACKET];
                    for k in 0..PACKET {
                        out[k] = va[k].floor();
                    }
                    *rf.get_mut(usize::from(dst)).ok_or("invalid reg")? = out;
                }
                PacketOp::FractF { dst, a } => {
                    let va = *rf.get(usize::from(a)).ok_or("invalid reg")?;
                    let mut out = [0.0f32; PACKET];
                    for k in 0..PACKET {
                        out[k] = va[k] - va[k].floor();
                    }
                    *rf.get_mut(usize::from(dst)).ok_or("invalid reg")? = out;
                }
                PacketOp::CeilF { dst, a } => {
                    let va = *rf.get(usize::from(a)).ok_or("invalid reg")?;
                    let mut out = [0.0f32; PACKET];
                    for k in 0..PACKET {
                        out[k] = va[k].ceil();
                    }
                    *rf.get_mut(usize::from(dst)).ok_or("invalid reg")? = out;
                }
                PacketOp::SqrtF { dst, a } => {
                    let va = *rf.get(usize::from(a)).ok_or("invalid reg")?;
                    let mut out = [0.0f32; PACKET];
                    for k in 0..PACKET {
                        out[k] = va[k].sqrt();
                    }
                    *rf.get_mut(usize::from(dst)).ok_or("invalid reg")? = out;
                }
                PacketOp::ExpF { dst, a } => {
                    let va = *rf.get(usize::from(a)).ok_or("invalid reg")?;
                    let mut out = [0.0f32; PACKET];
                    for k in 0..PACKET {
                        out[k] = va[k].exp();
                    }
                    *rf.get_mut(usize::from(dst)).ok_or("invalid reg")? = out;
                }
                PacketOp::LogF { dst, a } => {
                    let va = *rf.get(usize::from(a)).ok_or("invalid reg")?;
                    let mut out = [0.0f32; PACKET];
                    for k in 0..PACKET {
                        out[k] = va[k].ln();
                    }
                    *rf.get_mut(usize::from(dst)).ok_or("invalid reg")? = out;
                }
                PacketOp::Log2F { dst, a } => {
                    let va = *rf.get(usize::from(a)).ok_or("invalid reg")?;
                    let mut out = [0.0f32; PACKET];
                    for k in 0..PACKET {
                        out[k] = va[k].log2();
                    }
                    *rf.get_mut(usize::from(dst)).ok_or("invalid reg")? = out;
                }
                PacketOp::Pow2F { dst, a } => {
                    let va = *rf.get(usize::from(a)).ok_or("invalid reg")?;
                    let mut out = [0.0f32; PACKET];
                    for k in 0..PACKET {
                        out[k] = va[k].exp2();
                    }
                    *rf.get_mut(usize::from(dst)).ok_or("invalid reg")? = out;
                }
                PacketOp::SinF { dst, a } => {
                    let va = *rf.get(usize::from(a)).ok_or("invalid reg")?;
                    *rf.get_mut(usize::from(dst)).ok_or("invalid reg")? = sin_simd8(va);
                }
                PacketOp::CosF { dst, a } => {
                    let va = *rf.get(usize::from(a)).ok_or("invalid reg")?;
                    *rf.get_mut(usize::from(dst)).ok_or("invalid reg")? = cos_simd8(va);
                }
                PacketOp::TanF { dst, a } => {
                    let va = *rf.get(usize::from(a)).ok_or("invalid reg")?;
                    let mut out = [0.0f32; PACKET];
                    for k in 0..PACKET {
                        out[k] = va[k].tan();
                    }
                    *rf.get_mut(usize::from(dst)).ok_or("invalid reg")? = out;
                }
                PacketOp::AddF { dst, a, b } => {
                    let va = *rf.get(usize::from(a)).ok_or("invalid reg")?;
                    let vb = *rf.get(usize::from(b)).ok_or("invalid reg")?;
                    let mut out = [0.0f32; PACKET];
                    for k in 0..PACKET {
                        out[k] = va[k] + vb[k];
                    }
                    *rf.get_mut(usize::from(dst)).ok_or("invalid reg")? = out;
                }
                PacketOp::SubF { dst, a, b } => {
                    let va = *rf.get(usize::from(a)).ok_or("invalid reg")?;
                    let vb = *rf.get(usize::from(b)).ok_or("invalid reg")?;
                    let mut out = [0.0f32; PACKET];
                    for k in 0..PACKET {
                        out[k] = va[k] - vb[k];
                    }
                    *rf.get_mut(usize::from(dst)).ok_or("invalid reg")? = out;
                }
                PacketOp::MulF { dst, a, b } => {
                    let va = *rf.get(usize::from(a)).ok_or("invalid reg")?;
                    let vb = *rf.get(usize::from(b)).ok_or("invalid reg")?;
                    let mut out = [0.0f32; PACKET];
                    for k in 0..PACKET {
                        out[k] = va[k] * vb[k];
                    }
                    *rf.get_mut(usize::from(dst)).ok_or("invalid reg")? = out;
                }
                PacketOp::DivF { dst, a, b } => {
                    let va = *rf.get(usize::from(a)).ok_or("invalid reg")?;
                    let vb = *rf.get(usize::from(b)).ok_or("invalid reg")?;
                    let mut out = [0.0f32; PACKET];
                    for k in 0..PACKET {
                        out[k] = va[k] / vb[k];
                    }
                    *rf.get_mut(usize::from(dst)).ok_or("invalid reg")? = out;
                }
                PacketOp::ModF { dst, a, b } => {
                    let va = *rf.get(usize::from(a)).ok_or("invalid reg")?;
                    let vb = *rf.get(usize::from(b)).ok_or("invalid reg")?;
                    let mut out = [0.0f32; PACKET];
                    for k in 0..PACKET {
                        out[k] = va[k] - vb[k] * (va[k] / vb[k]).floor();
                    }
                    *rf.get_mut(usize::from(dst)).ok_or("invalid reg")? = out;
                }
                PacketOp::MinF { dst, a, b } => {
                    let va = *rf.get(usize::from(a)).ok_or("invalid reg")?;
                    let vb = *rf.get(usize::from(b)).ok_or("invalid reg")?;
                    let mut out = [0.0f32; PACKET];
                    for k in 0..PACKET {
                        out[k] = va[k].min(vb[k]);
                    }
                    *rf.get_mut(usize::from(dst)).ok_or("invalid reg")? = out;
                }
                PacketOp::MaxF { dst, a, b } => {
                    let va = *rf.get(usize::from(a)).ok_or("invalid reg")?;
                    let vb = *rf.get(usize::from(b)).ok_or("invalid reg")?;
                    let mut out = [0.0f32; PACKET];
                    for k in 0..PACKET {
                        out[k] = va[k].max(vb[k]);
                    }
                    *rf.get_mut(usize::from(dst)).ok_or("invalid reg")? = out;
                }
                PacketOp::PowF { dst, a, b } => {
                    let va = *rf.get(usize::from(a)).ok_or("invalid reg")?;
                    let vb = *rf.get(usize::from(b)).ok_or("invalid reg")?;
                    let mut out = [0.0f32; PACKET];
                    for k in 0..PACKET {
                        out[k] = va[k].powf(vb[k]);
                    }
                    *rf.get_mut(usize::from(dst)).ok_or("invalid reg")? = out;
                }
                PacketOp::Atan2F { dst, y, x } => {
                    let vy = *rf.get(usize::from(y)).ok_or("invalid reg")?;
                    let vx = *rf.get(usize::from(x)).ok_or("invalid reg")?;
                    let mut out = [0.0f32; PACKET];
                    for k in 0..PACKET {
                        out[k] = vy[k].atan2(vx[k]);
                    }
                    *rf.get_mut(usize::from(dst)).ok_or("invalid reg")? = out;
                }
                PacketOp::CartesianF { dst_x, dst_y, r, ang } => {
                    let vr = *rf.get(usize::from(r)).ok_or("invalid reg")?;
                    let va = *rf.get(usize::from(ang)).ok_or("invalid reg")?;
                    let mut ox = [0.0f32; PACKET];
                    let mut oy = [0.0f32; PACKET];
                    for k in 0..PACKET {
                        let (s, c) = va[k].sin_cos();
                        ox[k] = vr[k] * c;
                        oy[k] = vr[k] * s;
                    }
                    *rf.get_mut(usize::from(dst_x)).ok_or("invalid reg")? = ox;
                    *rf.get_mut(usize::from(dst_y)).ok_or("invalid reg")? = oy;
                }
                PacketOp::LerpF { dst, a, b, t } => {
                    let va = *rf.get(usize::from(a)).ok_or("invalid reg")?;
                    let vb = *rf.get(usize::from(b)).ok_or("invalid reg")?;
                    let vt = *rf.get(usize::from(t)).ok_or("invalid reg")?;
                    let mut out = [0.0f32; PACKET];
                    for k in 0..PACKET {
                        out[k] = va[k] + (vb[k] - va[k]) * vt[k];
                    }
                    *rf.get_mut(usize::from(dst)).ok_or("invalid reg")? = out;
                }
                PacketOp::EqF { dst, a, b } => {
                    let va = *rf.get(usize::from(a)).ok_or("invalid reg")?;
                    let vb = *rf.get(usize::from(b)).ok_or("invalid reg")?;
                    let mut out = [false; PACKET];
                    for k in 0..PACKET {
                        out[k] = va[k] == vb[k];
                    }
                    *rb.get_mut(usize::from(dst)).ok_or("invalid reg")? = out;
                }
                PacketOp::EqI { dst, a, b } => {
                    let va = *ri.get(usize::from(a)).ok_or("invalid reg")?;
                    let vb = *ri.get(usize::from(b)).ok_or("invalid reg")?;
                    let mut out = [false; PACKET];
                    for k in 0..PACKET {
                        out[k] = va[k] == vb[k];
                    }
                    *rb.get_mut(usize::from(dst)).ok_or("invalid reg")? = out;
                }
                PacketOp::EqB { dst, a, b } => {
                    let va = *rb.get(usize::from(a)).ok_or("invalid reg")?;
                    let vb = *rb.get(usize::from(b)).ok_or("invalid reg")?;
                    let mut out = [false; PACKET];
                    for k in 0..PACKET {
                        out[k] = va[k] == vb[k];
                    }
                    *rb.get_mut(usize::from(dst)).ok_or("invalid reg")? = out;
                }
                PacketOp::GtF { dst, a, b } => {
                    let va = *rf.get(usize::from(a)).ok_or("invalid reg")?;
                    let vb = *rf.get(usize::from(b)).ok_or("invalid reg")?;
                    let mut out = [false; PACKET];
                    for k in 0..PACKET {
                        out[k] = va[k] > vb[k];
                    }
                    *rb.get_mut(usize::from(dst)).ok_or("invalid reg")? = out;
                }
                PacketOp::GtEqF { dst, a, b } => {
                    let va = *rf.get(usize::from(a)).ok_or("invalid reg")?;
                    let vb = *rf.get(usize::from(b)).ok_or("invalid reg")?;
                    let mut out = [false; PACKET];
                    for k in 0..PACKET {
                        out[k] = va[k] >= vb[k];
                    }
                    *rb.get_mut(usize::from(dst)).ok_or("invalid reg")? = out;
                }
                PacketOp::LrF { dst, a, b } => {
                    let va = *rf.get(usize::from(a)).ok_or("invalid reg")?;
                    let vb = *rf.get(usize::from(b)).ok_or("invalid reg")?;
                    let mut out = [false; PACKET];
                    for k in 0..PACKET {
                        out[k] = va[k] < vb[k];
                    }
                    *rb.get_mut(usize::from(dst)).ok_or("invalid reg")? = out;
                }
                PacketOp::LrEqF { dst, a, b } => {
                    let va = *rf.get(usize::from(a)).ok_or("invalid reg")?;
                    let vb = *rf.get(usize::from(b)).ok_or("invalid reg")?;
                    let mut out = [false; PACKET];
                    for k in 0..PACKET {
                        out[k] = va[k] <= vb[k];
                    }
                    *rb.get_mut(usize::from(dst)).ok_or("invalid reg")? = out;
                }
                PacketOp::GtI { dst, a, b } => {
                    let va = *ri.get(usize::from(a)).ok_or("invalid reg")?;
                    let vb = *ri.get(usize::from(b)).ok_or("invalid reg")?;
                    let mut out = [false; PACKET];
                    for k in 0..PACKET {
                        out[k] = va[k] > vb[k];
                    }
                    *rb.get_mut(usize::from(dst)).ok_or("invalid reg")? = out;
                }
                PacketOp::GtEqI { dst, a, b } => {
                    let va = *ri.get(usize::from(a)).ok_or("invalid reg")?;
                    let vb = *ri.get(usize::from(b)).ok_or("invalid reg")?;
                    let mut out = [false; PACKET];
                    for k in 0..PACKET {
                        out[k] = va[k] >= vb[k];
                    }
                    *rb.get_mut(usize::from(dst)).ok_or("invalid reg")? = out;
                }
                PacketOp::LrI { dst, a, b } => {
                    let va = *ri.get(usize::from(a)).ok_or("invalid reg")?;
                    let vb = *ri.get(usize::from(b)).ok_or("invalid reg")?;
                    let mut out = [false; PACKET];
                    for k in 0..PACKET {
                        out[k] = va[k] < vb[k];
                    }
                    *rb.get_mut(usize::from(dst)).ok_or("invalid reg")? = out;
                }
                PacketOp::LrEqI { dst, a, b } => {
                    let va = *ri.get(usize::from(a)).ok_or("invalid reg")?;
                    let vb = *ri.get(usize::from(b)).ok_or("invalid reg")?;
                    let mut out = [false; PACKET];
                    for k in 0..PACKET {
                        out[k] = va[k] <= vb[k];
                    }
                    *rb.get_mut(usize::from(dst)).ok_or("invalid reg")? = out;
                }
                PacketOp::MinI { dst, a, b } => {
                    let va = *ri.get(usize::from(a)).ok_or("invalid reg")?;
                    let vb = *ri.get(usize::from(b)).ok_or("invalid reg")?;
                    let mut out = [0i32; PACKET];
                    for k in 0..PACKET {
                        out[k] = va[k].min(vb[k]);
                    }
                    *ri.get_mut(usize::from(dst)).ok_or("invalid reg")? = out;
                }
                PacketOp::MaxI { dst, a, b } => {
                    let va = *ri.get(usize::from(a)).ok_or("invalid reg")?;
                    let vb = *ri.get(usize::from(b)).ok_or("invalid reg")?;
                    let mut out = [0i32; PACKET];
                    for k in 0..PACKET {
                        out[k] = va[k].max(vb[k]);
                    }
                    *ri.get_mut(usize::from(dst)).ok_or("invalid reg")? = out;
                }
                PacketOp::IntToFloat { dst, a } => {
                    let va = *ri.get(usize::from(a)).ok_or("invalid reg")?;
                    let mut out = [0.0f32; PACKET];
                    for k in 0..PACKET {
                        out[k] = va[k] as f32;
                    }
                    *rf.get_mut(usize::from(dst)).ok_or("invalid reg")? = out;
                }
                PacketOp::And { dst, a, b } => {
                    let va = *rb.get(usize::from(a)).ok_or("invalid reg")?;
                    let vb = *rb.get(usize::from(b)).ok_or("invalid reg")?;
                    let mut out = [false; PACKET];
                    for k in 0..PACKET {
                        out[k] = va[k] && vb[k];
                    }
                    *rb.get_mut(usize::from(dst)).ok_or("invalid reg")? = out;
                }
                PacketOp::Or { dst, a, b } => {
                    let va = *rb.get(usize::from(a)).ok_or("invalid reg")?;
                    let vb = *rb.get(usize::from(b)).ok_or("invalid reg")?;
                    let mut out = [false; PACKET];
                    for k in 0..PACKET {
                        out[k] = va[k] || vb[k];
                    }
                    *rb.get_mut(usize::from(dst)).ok_or("invalid reg")? = out;
                }
                PacketOp::Not { dst, a } => {
                    let va = *rb.get(usize::from(a)).ok_or("invalid reg")?;
                    let mut out = [false; PACKET];
                    for k in 0..PACKET {
                        out[k] = !va[k];
                    }
                    *rb.get_mut(usize::from(dst)).ok_or("invalid reg")? = out;
                }
                PacketOp::SelectF { dst, c, t, e } => {
                    let vc = *rb.get(usize::from(c)).ok_or("invalid reg")?;
                    let vt = *rf.get(usize::from(t)).ok_or("invalid reg")?;
                    let ve = *rf.get(usize::from(e)).ok_or("invalid reg")?;
                    let mut out = [0.0f32; PACKET];
                    for k in 0..PACKET {
                        out[k] = if vc[k] { vt[k] } else { ve[k] };
                    }
                    *rf.get_mut(usize::from(dst)).ok_or("invalid reg")? = out;
                }
                PacketOp::SelectI { dst, c, t, e } => {
                    let vc = *rb.get(usize::from(c)).ok_or("invalid reg")?;
                    let vt = *ri.get(usize::from(t)).ok_or("invalid reg")?;
                    let ve = *ri.get(usize::from(e)).ok_or("invalid reg")?;
                    let mut out = [0i32; PACKET];
                    for k in 0..PACKET {
                        out[k] = if vc[k] { vt[k] } else { ve[k] };
                    }
                    *ri.get_mut(usize::from(dst)).ok_or("invalid reg")? = out;
                }
                PacketOp::SelectB { dst, c, t, e } => {
                    let vc = *rb.get(usize::from(c)).ok_or("invalid reg")?;
                    let vt = *rb.get(usize::from(t)).ok_or("invalid reg")?;
                    let ve = *rb.get(usize::from(e)).ok_or("invalid reg")?;
                    let mut out = [false; PACKET];
                    for k in 0..PACKET {
                        out[k] = if vc[k] { vt[k] } else { ve[k] };
                    }
                    *rb.get_mut(usize::from(dst)).ok_or("invalid reg")? = out;
                }
                PacketOp::SampleLum { dst, u, v, index, bilinear } => {
                    let vu = *rf.get(usize::from(u)).ok_or("invalid reg")?;
                    let vv = *rf.get(usize::from(v)).ok_or("invalid reg")?;
                    let mut out = [0.0f32; PACKET];
                    if let Some(image) = images.get(usize::from(index)).and_then(Option::as_ref) {
                        if !bilinear {
                            let w = image.width() as f32;
                            let h = image.height() as f32;
                            for k in 0..count {
                                out[k] = image.get((vu[k] * w).floor() as i64, (vv[k] * h).floor() as i64, tile)[0];
                            }
                        } else {
                            for k in 0..count {
                                out[k] = image.sample_stored(vu[k], vv[k], tile)[0];
                            }
                        }
                    }
                    *rf.get_mut(usize::from(dst)).ok_or("invalid reg")? = out;
                }
                PacketOp::SampleCol { dst, u, v, index, bilinear } => {
                    let vu = *rf.get(usize::from(u)).ok_or("invalid reg")?;
                    let vv = *rf.get(usize::from(v)).ok_or("invalid reg")?;
                    let mut o0 = [0.0f32; PACKET];
                    let mut o1 = [0.0f32; PACKET];
                    let mut o2 = [0.0f32; PACKET];
                    let mut o3 = [0.0f32; PACKET];
                    if let Some(image) = images.get(usize::from(index)).and_then(Option::as_ref) {
                        if !bilinear {
                            let w = image.width() as f32;
                            let h = image.height() as f32;
                            for k in 0..count {
                                let p = image.get((vu[k] * w).floor() as i64, (vv[k] * h).floor() as i64, tile);
                                o0[k] = p[0];
                                o1[k] = p[1];
                                o2[k] = p[2];
                                o3[k] = p[3];
                            }
                        } else {
                            for k in 0..count {
                                let p = image.sample_stored(vu[k], vv[k], tile);
                                o0[k] = p[0];
                                o1[k] = p[1];
                                o2[k] = p[2];
                                o3[k] = p[3];
                            }
                        }
                    }
                    *rf.get_mut(usize::from(dst[0])).ok_or("invalid reg")? = o0;
                    *rf.get_mut(usize::from(dst[1])).ok_or("invalid reg")? = o1;
                    *rf.get_mut(usize::from(dst[2])).ok_or("invalid reg")? = o2;
                    *rf.get_mut(usize::from(dst[3])).ok_or("invalid reg")? = o3;
                }
            }
        }

        // Convert output lanes to target row pixels.
        let integer =
            matches!(format, materialcraft_core::OutputFormat::U8 | materialcraft_core::OutputFormat::U16 | materialcraft_core::OutputFormat::Inherit);
        let dims = usize::from(self.output.dims);
        let mut lanes = [[0.0f32; PACKET]; 4];
        match self.output.kind {
            RegKind::Float => {
                for c in 0..dims {
                    lanes[c] = *rf.get(usize::from(self.output.slots[c])).ok_or("missing output")?;
                }
            }
            RegKind::Int => {
                for c in 0..dims {
                    let iv = *ri.get(usize::from(self.output.slots[c])).ok_or("missing output")?;
                    for k in 0..count {
                        lanes[c][k] = iv[k] as f32;
                    }
                }
            }
            RegKind::Bool => {
                let bv = *rb.get(usize::from(self.output.slots[0])).ok_or("missing output")?;
                for k in 0..count {
                    lanes[0][k] = if bv[k] { 1.0 } else { 0.0 };
                }
            }
        }

        for (k, target) in target_row.chunks_exact_mut(channels).take(count).enumerate() {
            let values = [lanes[0][k], lanes[1][k], lanes[2][k], lanes[3][k]];
            if !integer && values.iter().take(dims).any(|v| !v.is_finite()) {
                return Err("pixel function produced a non-finite output".into());
            }
            let px_out = if gray {
                let intensity = if dims == 4 { values[2].mul_add(1.0 / 3.0, values[0].mul_add(1.0 / 3.0, values[1] * (1.0 / 3.0))) } else { values[0] };
                [intensity, 0.0, 0.0, 0.0]
            } else {
                match dims {
                    1 => [values[0], values[0], values[0], 1.0],
                    2 => [values[0], values[1], 0.0, 1.0],
                    3 => [values[0], values[1], values[2], 1.0],
                    _ => values,
                }
            };
            target.copy_from_slice(px_out.get(..channels).ok_or("invalid output channels")?);
        }
        Ok(())
    }
}

fn eliminate_dead_ops(ops: &[PacketOp], output: RegRef, n_f: usize, n_i: usize, n_b: usize) -> Vec<PacketOp> {
    let mut live_f = vec![false; n_f];
    let mut live_i = vec![false; n_i];
    let mut live_b = vec![false; n_b];
    for c in 0..usize::from(output.dims) {
        let s = usize::from(output.slots[c]);
        match output.kind {
            RegKind::Float => {
                if let Some(flag) = live_f.get_mut(s) {
                    *flag = true;
                }
            }
            RegKind::Int => {
                if let Some(flag) = live_i.get_mut(s) {
                    *flag = true;
                }
            }
            RegKind::Bool => {
                if let Some(flag) = live_b.get_mut(s) {
                    *flag = true;
                }
            }
        }
    }

    let mut keep = vec![false; ops.len()];
    let mark_f = |lf: &mut [bool], s: u16| {
        if let Some(v) = lf.get_mut(usize::from(s)) {
            *v = true;
        }
    };
    let mark_i = |li: &mut [bool], s: u16| {
        if let Some(v) = li.get_mut(usize::from(s)) {
            *v = true;
        }
    };
    let mark_b = |lb: &mut [bool], s: u16| {
        if let Some(v) = lb.get_mut(usize::from(s)) {
            *v = true;
        }
    };

    for (idx, &op) in ops.iter().enumerate().rev() {
        let is_live = match op {
            PacketOp::NegF { dst, .. }
            | PacketOp::AbsF { dst, .. }
            | PacketOp::FloorF { dst, .. }
            | PacketOp::FractF { dst, .. }
            | PacketOp::CeilF { dst, .. }
            | PacketOp::SqrtF { dst, .. }
            | PacketOp::ExpF { dst, .. }
            | PacketOp::LogF { dst, .. }
            | PacketOp::Log2F { dst, .. }
            | PacketOp::Pow2F { dst, .. }
            | PacketOp::SinF { dst, .. }
            | PacketOp::CosF { dst, .. }
            | PacketOp::TanF { dst, .. }
            | PacketOp::AddF { dst, .. }
            | PacketOp::SubF { dst, .. }
            | PacketOp::MulF { dst, .. }
            | PacketOp::DivF { dst, .. }
            | PacketOp::ModF { dst, .. }
            | PacketOp::MinF { dst, .. }
            | PacketOp::MaxF { dst, .. }
            | PacketOp::PowF { dst, .. }
            | PacketOp::Atan2F { dst, .. }
            | PacketOp::LerpF { dst, .. }
            | PacketOp::SelectF { dst, .. }
            | PacketOp::IntToFloat { dst, .. }
            | PacketOp::SampleLum { dst, .. } => live_f.get(usize::from(dst)).copied().unwrap_or(false),
            PacketOp::CartesianF { dst_x, dst_y, .. } => {
                live_f.get(usize::from(dst_x)).copied().unwrap_or(false) || live_f.get(usize::from(dst_y)).copied().unwrap_or(false)
            }
            PacketOp::SampleCol { dst, .. } => dst.iter().any(|&s| live_f.get(usize::from(s)).copied().unwrap_or(false)),
            PacketOp::SelectI { dst, .. } | PacketOp::MinI { dst, .. } | PacketOp::MaxI { dst, .. } => live_i.get(usize::from(dst)).copied().unwrap_or(false),
            PacketOp::EqF { dst, .. }
            | PacketOp::EqI { dst, .. }
            | PacketOp::EqB { dst, .. }
            | PacketOp::GtF { dst, .. }
            | PacketOp::GtEqF { dst, .. }
            | PacketOp::LrF { dst, .. }
            | PacketOp::LrEqF { dst, .. }
            | PacketOp::GtI { dst, .. }
            | PacketOp::GtEqI { dst, .. }
            | PacketOp::LrI { dst, .. }
            | PacketOp::LrEqI { dst, .. }
            | PacketOp::And { dst, .. }
            | PacketOp::Or { dst, .. }
            | PacketOp::Not { dst, .. }
            | PacketOp::SelectB { dst, .. } => live_b.get(usize::from(dst)).copied().unwrap_or(false),
        };
        if !is_live {
            continue;
        }
        if let Some(k) = keep.get_mut(idx) {
            *k = true;
        }
        match op {
            PacketOp::NegF { a, .. }
            | PacketOp::AbsF { a, .. }
            | PacketOp::FloorF { a, .. }
            | PacketOp::FractF { a, .. }
            | PacketOp::CeilF { a, .. }
            | PacketOp::SqrtF { a, .. }
            | PacketOp::ExpF { a, .. }
            | PacketOp::LogF { a, .. }
            | PacketOp::Log2F { a, .. }
            | PacketOp::Pow2F { a, .. }
            | PacketOp::SinF { a, .. }
            | PacketOp::CosF { a, .. }
            | PacketOp::TanF { a, .. } => mark_f(&mut live_f, a),
            PacketOp::AddF { a, b, .. }
            | PacketOp::SubF { a, b, .. }
            | PacketOp::MulF { a, b, .. }
            | PacketOp::DivF { a, b, .. }
            | PacketOp::ModF { a, b, .. }
            | PacketOp::MinF { a, b, .. }
            | PacketOp::MaxF { a, b, .. }
            | PacketOp::PowF { a, b, .. }
            | PacketOp::EqF { a, b, .. }
            | PacketOp::GtF { a, b, .. }
            | PacketOp::GtEqF { a, b, .. }
            | PacketOp::LrF { a, b, .. }
            | PacketOp::LrEqF { a, b, .. } => {
                mark_f(&mut live_f, a);
                mark_f(&mut live_f, b);
            }
            PacketOp::Atan2F { y, x, .. } => {
                mark_f(&mut live_f, y);
                mark_f(&mut live_f, x);
            }
            PacketOp::CartesianF { r, ang, .. } => {
                mark_f(&mut live_f, r);
                mark_f(&mut live_f, ang);
            }
            PacketOp::LerpF { a, b, t, .. } => {
                mark_f(&mut live_f, a);
                mark_f(&mut live_f, b);
                mark_f(&mut live_f, t);
            }
            PacketOp::EqI { a, b, .. }
            | PacketOp::GtI { a, b, .. }
            | PacketOp::GtEqI { a, b, .. }
            | PacketOp::LrI { a, b, .. }
            | PacketOp::LrEqI { a, b, .. }
            | PacketOp::MinI { a, b, .. }
            | PacketOp::MaxI { a, b, .. } => {
                mark_i(&mut live_i, a);
                mark_i(&mut live_i, b);
            }
            PacketOp::IntToFloat { a, .. } => mark_i(&mut live_i, a),
            PacketOp::EqB { a, b, .. } | PacketOp::And { a, b, .. } | PacketOp::Or { a, b, .. } => {
                mark_b(&mut live_b, a);
                mark_b(&mut live_b, b);
            }
            PacketOp::Not { a, .. } => mark_b(&mut live_b, a),
            PacketOp::SelectF { c, t, e, .. } => {
                mark_b(&mut live_b, c);
                mark_f(&mut live_f, t);
                mark_f(&mut live_f, e);
            }
            PacketOp::SelectI { c, t, e, .. } => {
                mark_b(&mut live_b, c);
                mark_i(&mut live_i, t);
                mark_i(&mut live_i, e);
            }
            PacketOp::SelectB { c, t, e, .. } => {
                mark_b(&mut live_b, c);
                mark_b(&mut live_b, t);
                mark_b(&mut live_b, e);
            }
            PacketOp::SampleLum { u, v, .. } | PacketOp::SampleCol { u, v, .. } => {
                mark_f(&mut live_f, u);
                mark_f(&mut live_f, v);
            }
        }
    }

    ops.iter().zip(keep).filter_map(|(&op, k)| k.then_some(op)).collect()
}

// Preserve the interpreter's scalar arithmetic in every lane. Polynomial approximations
// can change stored F32 pixels even for small, finite inputs.
#[inline]
fn sin_simd8(values: [f32; PACKET]) -> [f32; PACKET] {
    values.map(f32::sin)
}

#[inline]
fn cos_simd8(values: [f32; PACKET]) -> [f32; PACKET] {
    values.map(f32::cos)
}

fn reg_type_code(kind: RegKind, dims: u8) -> u32 {
    match (kind, dims) {
        (RegKind::Bool, _) => materialcraft_core::ValueType::Boolean as u32,
        (RegKind::Int, 1) => 16,
        (RegKind::Int, 2) => 32,
        (RegKind::Int, 3) => 64,
        (RegKind::Int, 4) => 128,
        (RegKind::Float, 1) => 256,
        (RegKind::Float, 2) => 512,
        (RegKind::Float, 3) => 1024,
        (RegKind::Float, 4) => 2048,
        _ => 0,
    }
}

/// Mark every instruction reachable from a branch (`slot 1` or `slot 2`) of any `ifelse`.
fn conditional_instructions(instructions: &[Instruction]) -> Vec<bool> {
    let mut cond = vec![false; instructions.len()];
    let mut stack = Vec::new();
    for inst in instructions {
        if inst.definition.id == "ifelse" {
            if let Some(&t) = inst.inputs.get(1) {
                stack.push(t);
            }
            if let Some(&e) = inst.inputs.get(2) {
                stack.push(e);
            }
        }
    }
    while let Some(i) = stack.pop() {
        if let Some(flag) = cond.get_mut(i)
            && !*flag
        {
            *flag = true;
            if let Some(inst) = instructions.get(i) {
                stack.extend(&inst.inputs);
            }
        }
    }
    cond
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::function::{EvaluationContext, EvaluationState, FunctionScope, compile};
    use materialcraft_core::{FunctionGraph, FunctionNode, OutputFormat, Value};

    fn n(id: u32, op: &str, inputs: &[u32], value: Option<Value>) -> FunctionNode {
        FunctionNode {
            id,
            op: op.into(),
            inputs: inputs.iter().map(|&i| Some(i)).collect(),
            value,
            type_code: 0,
            pos: [0.0, 0.0],
            instance: None,
            source: None,
            missing: None,
            missing_data: String::new(),
        }
    }

    #[test]
    fn packet_trig_matches_interpreter_bits() {
        let values = [0.095_f32, -0.095, 0.12345, -0.0, 0.0, 4096.0, 4097.0, 1.0e9];
        let sine = sin_simd8(values);
        let cosine = cos_simd8(values);
        for (lane, value) in values.into_iter().enumerate() {
            assert_eq!(sine[lane].to_bits(), value.sin().to_bits(), "sin lane {lane}");
            assert_eq!(cosine[lane].to_bits(), value.cos().to_bits(), "cos lane {lane}");
        }
    }

    #[test]
    fn fast_evaluator_matches_interpreter_on_dag_with_hoisted_vars_and_ifelse() {
        // Compute: x = $pos.x; h = set("h", x * 3.0); y = (h > 1.5) ? (h - floor(h)) : (h * 0.5);
        // sequenced after `set`.
        let graph = FunctionGraph {
            nodes: vec![
                n(1, "get_float2", &[], Some(Value::Text("$pos".into()))),
                n(2, "swizzle1", &[1], Some(Value::Int(0))),
                n(3, "const_float1", &[], Some(Value::Float(3.0))),
                n(4, "mul", &[2, 3], None),
                n(5, "set", &[4], Some(Value::Text("h".into()))),
                n(6, "get_float1", &[], Some(Value::Text("h".into()))),
                n(7, "const_float1", &[], Some(Value::Float(1.5))),
                n(8, "gt", &[6, 7], None),
                n(9, "floor", &[6], None),
                n(10, "sub", &[6, 9], None),
                n(11, "const_float1", &[], Some(Value::Float(0.5))),
                n(12, "mul", &[6, 11], None),
                n(13, "ifelse", &[8, 10, 12], None),
                n(14, "sequence", &[5, 13], None),
            ],
            output: Some(14),
            ..FunctionGraph::default()
        };
        let program = compile(&graph, 0).expect("compile");
        let vars = BTreeMap::new();
        let fast = FastProgram::try_compile(&program, &vars, [64.0, 64.0], true).expect("fast program");
        let mut regs = fast.new_regs();
        let mut state = EvaluationState::default();
        for chunk in 0..2 {
            let mut px = [0.0f32; PACKET];
            for k in 0..PACKET {
                px[k] = ((chunk * PACKET + k) as f32 + 0.5) / 16.0;
            }
            let mut out = [0.0f32; PACKET];
            let pctx = PacketCtx { images: &[], tile: true, gray: true, format: OutputFormat::U16, channels: 1 };
            fast.eval_packet(&mut regs, px, 0.25, &pctx, PACKET, &mut out).expect("fast");
            for (k, &got) in out.iter().enumerate() {
                let ctx = EvaluationContext {
                    scope: FunctionScope::PixelProcessor,
                    position: [px[k], 0.25],
                    size: [64.0, 64.0],
                    images: &[],
                    tile: true,
                    variables: &vars,
                };
                let ref_val = program.evaluate_pixel(&ctx, &mut state).expect("ref");
                assert_eq!(ref_val.as_f32(), Some(got), "mismatch at step {}", chunk * PACKET + k);
            }
        }
    }

    #[test]
    fn simd8_sin_and_cos_match_std_within_one_micro() {
        let mut max_err = 0.0f32;
        for step in -2000..=2000 {
            let base = step as f32 * 2.047;
            let mut v = [0.0f32; PACKET];
            for k in 0..PACKET {
                v[k] = base + k as f32 * 0.137;
            }
            let s = sin_simd8(v);
            let c = cos_simd8(v);
            for k in 0..PACKET {
                max_err = max_err.max((s[k] - v[k].sin()).abs()).max((c[k] - v[k].cos()).abs());
            }
        }
        assert!(max_err < 1e-6, "simd8 trig max error too large: {max_err}");
    }

    #[test]
    fn fast_compiler_respects_tile_system_vars_bool_and_float_output_contract() {
        let vars = BTreeMap::new();

        // 1. `$tiling` reflects `tile: false` (0) vs `tile: true` (3).
        let tiling_graph = FunctionGraph {
            nodes: vec![
                n(1, "get_integer1", &[], Some(Value::Text("$tiling".into()))),
                n(2, "const_int1", &[], Some(Value::Int(0))),
                n(3, "eq", &[1, 2], None),
                n(4, "const_float1", &[], Some(Value::Float(1.0))),
                n(5, "const_float1", &[], Some(Value::Float(0.25))),
                n(6, "ifelse", &[3, 4, 5], None),
            ],
            output: Some(6),
            ..FunctionGraph::default()
        };
        let tiling_prog = compile(&tiling_graph, 0).expect("compile");
        let pctx = PacketCtx { images: &[], tile: false, gray: true, format: OutputFormat::F32, channels: 1 };
        let fast_off = FastProgram::try_compile(&tiling_prog, &vars, [8.0, 8.0], false).expect("tile=false");
        let mut out_off = [0.0f32; 1];
        fast_off.eval_packet(&mut fast_off.new_regs(), [0.5; PACKET], 0.5, &pctx, 1, &mut out_off).expect("eval");
        assert_eq!(out_off[0], 1.0);

        let fast_on = FastProgram::try_compile(&tiling_prog, &vars, [8.0, 8.0], true).expect("tile=true");
        let mut out_on = [0.0f32; 1];
        fast_on.eval_packet(&mut fast_on.new_regs(), [0.5; PACKET], 0.5, &pctx, 1, &mut out_on).expect("eval");
        assert_eq!(out_on[0], 0.25);

        // 2. Missing `$`-prefixed variable declines fast compilation so interpreter errors.
        let missing_sys =
            FunctionGraph { nodes: vec![n(1, "get_float1", &[], Some(Value::Text("$missing".into())))], output: Some(1), ..FunctionGraph::default() };
        let missing_prog = compile(&missing_sys, 0).expect("compile");
        assert!(FastProgram::try_compile(&missing_prog, &vars, [8.0, 8.0], true).is_none());

        // 3. `get_bool` coerces Int for user vars, but NOT for `$`-prefixed vars.
        let bool_coerce = |name: &str| FunctionGraph {
            nodes: vec![
                n(1, "get_bool", &[], Some(Value::Text(name.into()))),
                n(2, "const_float1", &[], Some(Value::Float(1.0))),
                n(3, "const_float1", &[], Some(Value::Float(0.0))),
                n(4, "ifelse", &[1, 2, 3], None),
            ],
            output: Some(4),
            ..FunctionGraph::default()
        };
        let mut custom_vars = BTreeMap::new();
        custom_vars.insert("$custom".to_string(), Value::Int(7));
        custom_vars.insert("enabled".to_string(), Value::Int(7));
        let sys_bool_prog = compile(&bool_coerce("$custom"), 0).expect("compile");
        assert!(FastProgram::try_compile(&sys_bool_prog, &custom_vars, [8.0, 8.0], true).is_none());
        let user_bool_prog = compile(&bool_coerce("enabled"), 0).expect("compile");
        let fast_user = FastProgram::try_compile(&user_bool_prog, &custom_vars, [8.0, 8.0], true).expect("user int->bool");
        let mut out_user = [0.0f32; 1];
        fast_user.eval_packet(&mut fast_user.new_regs(), [0.5; PACKET], 0.5, &pctx, 1, &mut out_user).expect("eval");
        assert_eq!(out_user[0], 1.0);

        // 4. Non-float final output declines fast compilation.
        let int_out = FunctionGraph { nodes: vec![n(1, "const_int1", &[], Some(Value::Int(1)))], output: Some(1), ..FunctionGraph::default() };
        let int_prog = compile(&int_out, 0).expect("compile");
        assert!(FastProgram::try_compile(&int_prog, &vars, [8.0, 8.0], true).is_none());

        // 5. Hoisted `set` + `get_bool` compiles cleanly using ValueType::Boolean (4).
        let set_bool = FunctionGraph {
            nodes: vec![
                n(1, "const_bool", &[], Some(Value::Bool(true))),
                n(2, "set", &[1], Some(Value::Text("flag".into()))),
                n(3, "get_bool", &[], Some(Value::Text("flag".into()))),
                n(4, "const_float1", &[], Some(Value::Float(0.75))),
                n(5, "const_float1", &[], Some(Value::Float(0.25))),
                n(6, "ifelse", &[3, 4, 5], None),
                n(7, "sequence", &[2, 6], None),
            ],
            output: Some(7),
            ..FunctionGraph::default()
        };
        let set_bool_prog = compile(&set_bool, 0).expect("compile");
        let fast_bool = FastProgram::try_compile(&set_bool_prog, &vars, [8.0, 8.0], true).expect("set+get_bool compiles");
        let mut out_bool = [0.0f32; 1];
        fast_bool.eval_packet(&mut fast_bool.new_regs(), [0.5; PACKET], 0.5, &pctx, 1, &mut out_bool).expect("eval");
        assert_eq!(out_bool[0], 0.75);
    }

    #[test]
    fn fast_casts_floor_negative_constants_and_decline_invalid_or_dynamic_float_to_int() {
        let vars = BTreeMap::new();
        let pctx = PacketCtx { images: &[], tile: true, gray: true, format: OutputFormat::F32, channels: 1 };

        // 1. tofloat(toint1(-0.5)) floors to -1.0 (not 0.0).
        let floor_cast = FunctionGraph {
            nodes: vec![n(1, "const_float1", &[], Some(Value::Float(-0.5))), n(2, "toint1", &[1], None), n(3, "tofloat", &[2], None)],
            output: Some(3),
            ..FunctionGraph::default()
        };
        let prog = compile(&floor_cast, 0).expect("compile");
        let fast = FastProgram::try_compile(&prog, &vars, [8.0, 8.0], true).expect("const floor cast compiles");
        let mut out = [0.0f32; 1];
        fast.eval_packet(&mut fast.new_regs(), [0.5; PACKET], 0.5, &pctx, 1, &mut out).expect("eval");
        assert_eq!(out[0], -1.0);

        // 2. Identity casts (tofloat on float, toint1 on int) decline fast compilation.
        let id_float = FunctionGraph {
            nodes: vec![n(1, "const_float1", &[], Some(Value::Float(0.25))), n(2, "tofloat", &[1], None)],
            output: Some(2),
            ..FunctionGraph::default()
        };
        assert!(FastProgram::try_compile(&compile(&id_float, 0).expect("compile"), &vars, [8.0, 8.0], true).is_none());

        let id_int = FunctionGraph {
            nodes: vec![n(1, "const_int1", &[], Some(Value::Int(7))), n(2, "toint1", &[1], None), n(3, "tofloat", &[2], None)],
            output: Some(3),
            ..FunctionGraph::default()
        };
        assert!(FastProgram::try_compile(&compile(&id_int, 0).expect("compile"), &vars, [8.0, 8.0], true).is_none());

        // 3. Out-of-range constant and dynamic float-to-int casts decline fast compilation.
        let oob = FunctionGraph {
            nodes: vec![n(1, "const_float1", &[], Some(Value::Float(2_147_483_648.0))), n(2, "toint1", &[1], None), n(3, "tofloat", &[2], None)],
            output: Some(3),
            ..FunctionGraph::default()
        };
        assert!(FastProgram::try_compile(&compile(&oob, 0).expect("compile"), &vars, [8.0, 8.0], true).is_none());

        let dyn_cast = FunctionGraph {
            nodes: vec![
                n(1, "get_float2", &[], Some(Value::Text("$pos".into()))),
                n(2, "swizzle1", &[1], Some(Value::Int(0))),
                n(3, "toint1", &[2], None),
                n(4, "tofloat", &[3], None),
            ],
            output: Some(4),
            ..FunctionGraph::default()
        };
        assert!(FastProgram::try_compile(&compile(&dyn_cast, 0).expect("compile"), &vars, [8.0, 8.0], true).is_none());
    }

    #[test]
    fn simd8_trig_is_lane_independent_and_preserves_negative_zero() {
        let probe = 4.712_389_f32;
        let all_in_range = [probe; PACKET];
        let mut mixed = [probe; PACKET];
        mixed[7] = 1.0e9;
        mixed[6] = f32::NAN;
        assert_eq!(cos_simd8(all_in_range)[0].to_bits(), cos_simd8(mixed)[0].to_bits());
        assert_eq!(sin_simd8(all_in_range)[0].to_bits(), sin_simd8(mixed)[0].to_bits());
        assert!(cos_simd8(mixed)[6].is_nan());
        assert_eq!(cos_simd8(mixed)[7].to_bits(), 1.0e9_f32.cos().to_bits());

        let mut zeros = [0.0f32; PACKET];
        zeros[0] = -0.0;
        zeros[1] = 0.0;
        let s = sin_simd8(zeros);
        assert_eq!(s[0].to_bits(), (-0.0f32).to_bits());
        assert_eq!(s[1].to_bits(), (0.0f32).to_bits());
    }
}
