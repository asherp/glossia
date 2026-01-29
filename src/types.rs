#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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
}

#[derive(Clone, Debug)]
pub enum Sym {
    NT(String),
    T(Pos),
    Opt(Box<Sym>),
}
