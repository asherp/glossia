#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
pub enum Pos {
    Det,
    Adj,
    N,
    V,
    Modal,
    Aux,
    Cop,
    To,
    Prep,
    Adv,
    Conj,
    Dot,
    Prefix,
    Pron,
}

impl Pos {
    /// All POS variants in enum order.
    pub const ALL: &'static [Pos] = &[
        Pos::Det, Pos::Adj, Pos::N, Pos::V, Pos::Modal, Pos::Aux,
        Pos::Cop, Pos::To, Pos::Prep, Pos::Adv, Pos::Conj, Pos::Dot,
        Pos::Prefix, Pos::Pron,
    ];

    /// Canonical Pos → string conversion (single source of truth).
    pub fn as_str(&self) -> &'static str {
        match self {
            Pos::Det => "Det",
            Pos::Adj => "Adj",
            Pos::N => "N",
            Pos::V => "V",
            Pos::Modal => "Modal",
            Pos::Aux => "Aux",
            Pos::Cop => "Cop",
            Pos::To => "To",
            Pos::Prep => "Prep",
            Pos::Adv => "Adv",
            Pos::Conj => "Conj",
            Pos::Dot => "Dot",
            Pos::Prefix => "Prefix",
            Pos::Pron => "Pron",
        }
    }

    /// Canonical string → Pos conversion (single source of truth).
    pub fn from_str(s: &str) -> Option<Pos> {
        match s {
            "Det" => Some(Pos::Det),
            "Adj" => Some(Pos::Adj),
            "N" => Some(Pos::N),
            "V" => Some(Pos::V),
            "Modal" => Some(Pos::Modal),
            "Aux" => Some(Pos::Aux),
            "Cop" => Some(Pos::Cop),
            "To" => Some(Pos::To),
            "Prep" => Some(Pos::Prep),
            "Adv" => Some(Pos::Adv),
            "Conj" => Some(Pos::Conj),
            "Dot" => Some(Pos::Dot),
            "Prefix" => Some(Pos::Prefix),
            "Pron" => Some(Pos::Pron),
            _ => None,
        }
    }

    /// Descriptive English name for display in statistics.
    pub fn description(&self) -> &'static str {
        match self {
            Pos::Det => "Determiners",
            Pos::Adj => "Adjectives",
            Pos::N => "Nouns",
            Pos::V => "Verbs",
            Pos::Modal => "Modals",
            Pos::Aux => "Auxiliaries",
            Pos::Cop => "Copulas",
            Pos::To => "To",
            Pos::Prep => "Prepositions",
            Pos::Adv => "Adverbs",
            Pos::Conj => "Conjunctions",
            Pos::Dot => "Punctuation",
            Pos::Prefix => "Prefixes",
            Pos::Pron => "Pronouns",
        }
    }
}

impl std::fmt::Display for Pos {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Debug)]
pub enum Sym {
    NT(String),
    T(Pos),
    Opt(Box<Sym>),
}
