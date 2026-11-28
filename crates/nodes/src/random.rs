#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct FunctionRandom {
    state: u32,
    multiplier: u32,
    increment: u32,
    uniform: bool,
}

impl FunctionRandom {
    pub(crate) fn uniform(state: u32) -> Self {
        Self { state, multiplier: 16_294_893, increment: 10_493_233, uniform: true }
    }

    pub(crate) fn image(state: u32) -> Self {
        Self { state, multiplier: 3_626_811, increment: 51_137, uniform: false }
    }

    /// Jenkins one-at-a-time mixing over the graph seed and the two normalized F32 coordinate words.
    pub(crate) fn pixel(seed: u32, position: [f32; 2]) -> Self {
        let mut hash = 0_u32;
        for word in [seed, position[0].to_bits(), position[1].to_bits()] {
            let mixed = hash.wrapping_add(word).wrapping_mul(1025);
            hash = mixed ^ (mixed >> 6);
        }
        let mixed = hash.wrapping_mul(9);
        Self::image((mixed ^ (mixed >> 11)).wrapping_mul(32769))
    }

    /// Each of these pattern properties starts its own sequence at the traversal seed.
    pub(crate) fn pattern(state: u32, property: &str) -> Option<Self> {
        let (multiplier, increment) = match property {
            "opacity" => return Some(Self::image(state)),
            "branchoffset" => (14_154_961, 50_989),
            "frameoffset" => (1_823_937, 50_707),
            "patternsize" => (5_812_301, 50_411),
            "patternrotation" => (3_470_325, 50_131),
            "patternsuppl" => (12_382_630, 50_647),
            "numberadded" => (9_501_597, 50_591),
            "switch" => (14_464_988, 50_111),
            _ => return None,
        };
        Some(Self { state, multiplier, increment, uniform: false })
    }

    pub(crate) fn draw(&mut self) -> f32 {
        self.state = self.state.wrapping_mul(self.multiplier).wrapping_add(self.increment) & 0x00ff_ffff;
        if self.uniform { self.state as f32 / 16_777_216.0 } else { (self.state >> 1) as f32 / 8_388_608.0 }
    }
}

impl Default for FunctionRandom {
    fn default() -> Self {
        Self::image(0)
    }
}

pub(crate) fn iteration_child(state: u32) -> u32 {
    state.wrapping_mul(8_532_665).wrapping_add(14_533) & 0x00ff_ffff
}

/// Quadrant child streams are independent of parameter-function draws.
/// Original constant-output probes establish a sixteen-depth repeating sequence.
pub(crate) fn quadrant_child(state: &mut u32, depth: u32) -> u32 {
    let (multiplier, increment) = match depth & 15 {
        1 => (1_691_414_u32, 52_627_u32),
        2 => (5_547_014, 52_673),
        3 => (1_289_321, 52_711),
        4 => (10_611_682, 52_747),
        5 => (2_603_697, 52_807),
        6 => (13_013_189, 52_859),
        7 => (14_482_087, 52_249),
        8 => (4_839_483, 52_289),
        9 => (3_906_728, 52_321),
        10 => (4_193_611, 52_379),
        11 => (4_656_553, 52_453),
        12 => (3_558_434, 52_511),
        13 => (7_796_889, 52_543),
        14 => (3_426_494, 52_189),
        15 => (11_525_242, 51_929),
        _ => (14_449_176, 52_571),
    };
    *state = state.wrapping_mul(multiplier).wrapping_add(increment) & 0x00ff_ffff;
    *state
}
