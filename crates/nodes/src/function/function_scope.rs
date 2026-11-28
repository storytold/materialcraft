/// The caller determines which context-dependent variables a function can read.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FunctionScope {
    #[default]
    Node,
    /// A graph or node's base random seed, evaluated as a 32-bit word.
    RandomSeed,
    ValueProcessor,
    PixelProcessor,
    FxMap,
    /// A pattern node's seed can read traversal variables and uses seed arithmetic.
    FxMapRandomSeed,
}

impl FunctionScope {
    pub(super) fn is_pattern(self) -> bool {
        matches!(self, Self::FxMap | Self::FxMapRandomSeed)
    }

    pub(super) fn uses_seed_words(self) -> bool {
        matches!(self, Self::RandomSeed | Self::FxMapRandomSeed)
    }
}
