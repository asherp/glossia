//! Reed–Solomon over GF(2^m), with the payload word as the symbol.
//!
//! One mistranscribed word is one wrong symbol. That is the point of working in
//! the wordlist's own field rather than over the bytes the words pack into: an
//! 11-bit word straddles two or three byte boundaries, so a byte-oriented code
//! spends two or three symbols of parity repairing a single fault.
//!
//! # Errors and erasures
//!
//! An *error* is a wrong symbol at an unknown position; an *erasure* is a
//! missing symbol at a known one. Locating damage halves its cost — with
//! `parity` parity symbols the code corrects any combination satisfying
//!
//! ```text
//! 2·errors + erasures ≤ parity
//! ```
//!
//! which is why [`crate::align`] earns its place: it turns damage a decoder
//! would have to search for into damage it is simply told about. A word mangled
//! off the wordlist is invisible to the harvest, so without alignment it is not
//! even an error — it is a length change that desynchronizes everything after
//! it, and no positional code survives that.
//!
//! # Cost
//!
//! Every parity symbol is another payload word in the prose. Correcting `t`
//! substitutions costs `2t` words; `t` erasures costs `t`.
//!
//! # Shortened codes
//!
//! A natural RS codeword over GF(2^m) is `2^m − 1` symbols. Real payloads are far
//! shorter — a 32-byte hash is 24 words in GF(2¹¹) — so this is a *shortened*
//! code: conceptually the codeword is padded with leading zeros that are never
//! transmitted. Positions are numbered within the actual length, consistently on
//! both sides, which is what keeps that equivalence exact.

use std::sync::Arc;

use crate::gf::{Gf, GfError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RsError {
    /// The field could not be built for this symbol width.
    Field(GfError),
    /// A codeword cannot be longer than the field's multiplicative group.
    TooLong { len: usize, max: usize },
    /// Parity must leave room for at least one message symbol.
    ParityTooLarge { parity: usize, len: usize },
    /// A symbol is not an element of the field — a word index past the end of
    /// the wordlist it was supposed to come from.
    SymbolOutOfField { symbol: u16, order: usize },
    /// An erasure position lies outside the codeword.
    ErasureOutOfRange { position: usize, len: usize },
    /// More erasures than parity can cover, before any error is even considered.
    TooManyErasures { erasures: usize, parity: usize },
    /// The damage exceeded what this parity can repair. Reported rather than
    /// guessed at: a decoder that returns its best effort here is the classic
    /// way a burst beyond the bound becomes a valid-looking wrong answer.
    Uncorrectable,
}

impl std::fmt::Display for RsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RsError::Field(e) => write!(f, "{e}"),
            RsError::TooLong { len, max } => {
                write!(f, "a codeword of {len} symbols exceeds the field's limit of {max}")
            }
            RsError::ParityTooLarge { parity, len } => write!(
                f,
                "{parity} parity symbols leave no message in a {len}-symbol codeword"
            ),
            RsError::SymbolOutOfField { symbol, order } => {
                write!(f, "symbol {symbol} is not an element of a field of order {order}")
            }
            RsError::ErasureOutOfRange { position, len } => {
                write!(f, "erasure at {position} lies outside a codeword of {len}")
            }
            RsError::TooManyErasures { erasures, parity } => write!(
                f,
                "{erasures} erasures exceed {parity} parity symbols"
            ),
            RsError::Uncorrectable => write!(
                f,
                "the damage exceeds what this parity can repair"
            ),
        }
    }
}

impl std::error::Error for RsError {}

impl From<GfError> for RsError {
    fn from(e: GfError) -> Self {
        RsError::Field(e)
    }
}

/// What a decode found and fixed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Corrected {
    /// The repaired codeword, parity included.
    pub codeword: Vec<u16>,
    /// Positions that held a wrong symbol at an unknown location.
    pub errors: Vec<usize>,
    /// Erasure positions that were filled in.
    pub erasures: Vec<usize>,
}

impl Corrected {
    /// Total symbols changed.
    pub fn repairs(&self) -> usize {
        self.errors.len() + self.erasures.len()
    }
}

/// A Reed–Solomon code: a field, and how many parity symbols to spend.
///
/// Polynomials are held **descending** where they are codeword-shaped (index 0
/// is the leading coefficient, matching how symbols are laid out in the prose)
/// and **ascending** where they are locator-shaped (index = power of x, so the
/// constant term leads). Each convention is the natural one for its use; the
/// helpers below are named for which they take.
#[derive(Debug, Clone)]
pub struct Rs {
    gf: Arc<Gf>,
    parity: usize,
    /// The generator, descending, degree `parity`.
    generator: Vec<u16>,
}

impl Rs {
    /// A code over GF(2^m) spending `parity` symbols.
    pub fn new(m: u32, parity: usize) -> Result<Self, RsError> {
        Ok(Rs::with_field(Gf::cached(m)?, parity))
    }

    /// A code over the field a wordlist of `len` words defines.
    pub fn for_wordlist_len(len: usize, parity: usize) -> Result<Self, RsError> {
        Ok(Rs::with_field(Gf::for_wordlist_len(len)?, parity))
    }

    pub fn with_field(gf: Arc<Gf>, parity: usize) -> Self {
        let generator = build_generator(&gf, parity);
        Rs { gf, parity, generator }
    }

    pub fn parity(&self) -> usize {
        self.parity
    }

    pub fn field(&self) -> &Arc<Gf> {
        &self.gf
    }

    /// The most symbols a codeword may hold in this field.
    pub fn max_len(&self) -> usize {
        self.gf.group_order()
    }

    /// How many errors this parity corrects when `erasures` positions are
    /// already known: the `2e + f ≤ parity` budget, solved for `e`.
    pub fn correctable_errors(&self, erasures: usize) -> usize {
        self.parity.saturating_sub(erasures) / 2
    }

    /// Append parity to `message`, returning `message || parity`.
    ///
    /// Systematic: the message symbols are left exactly where they were, so the
    /// words a reader already sees are unchanged and the parity is a suffix.
    pub fn encode(&self, message: &[u16]) -> Result<Vec<u16>, RsError> {
        let len = message.len() + self.parity;
        if len > self.max_len() {
            return Err(RsError::TooLong { len, max: self.max_len() });
        }
        if message.is_empty() {
            return Err(RsError::ParityTooLarge { parity: self.parity, len });
        }
        self.check_symbols(message)?;

        let mut out = vec![0u16; len];
        out[..message.len()].copy_from_slice(message);

        // Synthetic division of message·x^parity by the generator. The running
        // remainder is kept in place, which scribbles over the message region as
        // it goes; the original is restored afterwards.
        for i in 0..message.len() {
            let coef = out[i];
            if coef != 0 {
                for j in 1..=self.parity {
                    out[i + j] = self.gf.add(out[i + j], self.gf.mul(self.generator[j], coef));
                }
            }
        }
        out[..message.len()].copy_from_slice(message);
        Ok(out)
    }

    /// Whether `codeword` is a codeword — every syndrome zero.
    pub fn is_codeword(&self, codeword: &[u16]) -> bool {
        match self.syndromes(codeword) {
            Ok(s) => s.iter().all(|&v| v == 0),
            Err(_) => false,
        }
    }

    /// Repair `received`, given whatever positions are already known bad.
    ///
    /// An erasure's symbol value is ignored, so a caller with nothing to put
    /// there may pass any placeholder — [`crate::align::Alignment::payload_slots`]
    /// holds `None` at exactly these positions.
    ///
    /// Returns [`RsError::Uncorrectable`] rather than a best effort when the
    /// damage exceeds the bound. Every proposed repair is verified by
    /// recomputing the syndromes over the corrected word, so a decode that
    /// converges on a wrong-but-valid codeword is caught here rather than handed
    /// back as though it were the message.
    pub fn decode(&self, received: &[u16], erasures: &[usize]) -> Result<Corrected, RsError> {
        let n = received.len();
        if n > self.max_len() {
            return Err(RsError::TooLong { len: n, max: self.max_len() });
        }
        if n <= self.parity {
            return Err(RsError::ParityTooLarge { parity: self.parity, len: n });
        }
        self.check_symbols(received)?;

        // Erasures are deduplicated: the same slot named twice is one hole, and
        // counting it twice would refuse work that is well inside the budget.
        let mut erased: Vec<usize> = erasures.to_vec();
        erased.sort_unstable();
        erased.dedup();
        if let Some(&p) = erased.iter().find(|&&p| p >= n) {
            return Err(RsError::ErasureOutOfRange { position: p, len: n });
        }
        if erased.len() > self.parity {
            return Err(RsError::TooManyErasures {
                erasures: erased.len(),
                parity: self.parity,
            });
        }

        let mut work = received.to_vec();
        // An erasure's value is unknown by definition. Zeroing makes that
        // explicit rather than letting a placeholder influence the syndromes.
        for &p in &erased {
            work[p] = 0;
        }

        let syndromes = self.syndromes(&work)?;
        if syndromes.iter().all(|&v| v == 0) {
            // Already a codeword. With erasures marked this is still possible —
            // a slot may have been erased whose true symbol was zero.
            return Ok(Corrected {
                codeword: work,
                errors: Vec::new(),
                erasures: if erased.is_empty() { Vec::new() } else { erased },
            });
        }

        // The erasure locator: one root per known-bad position.
        let mut gamma = vec![1u16]; // ascending
        for &p in &erased {
            let y = self.gf.exp_of((n - 1 - p) as i64);
            gamma = self.poly_mul_asc(&gamma, &[1, y]);
        }

        // Forney-modified syndromes fold what is already known about the
        // erasures into the sequence Berlekamp–Massey searches, so BM has only
        // the unlocated errors left to find.
        //
        // The window matters: it is coefficients `f..parity` of S·Γ, not the
        // first `parity − f`. The leading f coefficients are contaminated by the
        // erasure locator's own low-order terms and satisfy no recurrence, so
        // feeding them to BM yields a locator that passes its own arithmetic and
        // names the wrong positions. What remains is exactly `parity − f` terms,
        // which is the budget — the same arithmetic from the other side.
        let modified = self.poly_mul_asc(&syndromes, &gamma);
        let window_end = self.parity.min(modified.len());
        let forney = &modified[erased.len().min(window_end)..window_end];
        let lambda_errors = self.berlekamp_massey(forney);

        // The full locator names every damaged position, found and given alike.
        let lambda = self.poly_mul_asc(&lambda_errors, &gamma);
        let degree = lambda.len().saturating_sub(1);
        if degree > self.parity || degree == 0 {
            return Err(RsError::Uncorrectable);
        }

        let positions = self.chien_search(&lambda, n);
        if positions.len() != degree {
            // The locator claims more roots than the codeword has positions for.
            // That means the damage is past the bound and the algebra has landed
            // somewhere meaningless.
            return Err(RsError::Uncorrectable);
        }
        if positions.len() > erased.len() + self.correctable_errors(erased.len()) {
            return Err(RsError::Uncorrectable);
        }

        let omega = {
            let mut o = self.poly_mul_asc(&syndromes, &lambda);
            o.truncate(self.parity);
            o
        };
        self.forney(&mut work, &lambda, &omega, &positions, n)?;

        // The proof. A repaired word that is not a codeword means the decode
        // converged on something the syndromes do not actually endorse, which is
        // exactly how a beyond-bound burst produces a plausible wrong answer.
        let check = self.syndromes(&work)?;
        if check.iter().any(|&v| v != 0) {
            return Err(RsError::Uncorrectable);
        }

        let erased_set: std::collections::HashSet<usize> = erased.iter().copied().collect();
        let errors: Vec<usize> = positions
            .iter()
            .copied()
            .filter(|p| !erased_set.contains(p))
            .collect();
        if errors.len() > self.correctable_errors(erased.len()) {
            return Err(RsError::Uncorrectable);
        }

        Ok(Corrected { codeword: work, errors, erasures: erased })
    }

    /// Strip the parity suffix, returning the message.
    pub fn message_of<'a>(&self, codeword: &'a [u16]) -> &'a [u16] {
        &codeword[..codeword.len() - self.parity]
    }

    // ── internals ──────────────────────────────────────────────────────────

    fn check_symbols(&self, symbols: &[u16]) -> Result<(), RsError> {
        let order = self.gf.order();
        if let Some(&s) = symbols.iter().find(|&&s| (s as usize) >= order) {
            return Err(RsError::SymbolOutOfField { symbol: s, order });
        }
        Ok(())
    }

    /// `S_j = r(α^j)` for `j` in `0..parity`, returned ascending.
    ///
    /// The received symbols are read as a polynomial in descending powers, so
    /// position `i` carries `x^(n-1-i)` and damage at `i` shows up with locator
    /// root `α^(n-1-i)`. Every step below uses that same numbering.
    fn syndromes(&self, received: &[u16]) -> Result<Vec<u16>, RsError> {
        self.check_symbols(received)?;
        Ok((0..self.parity)
            .map(|j| {
                let x = self.gf.exp_of(j as i64);
                received.iter().fold(0u16, |acc, &c| self.gf.add(self.gf.mul(acc, x), c))
            })
            .collect())
    }

    /// Multiply two ascending polynomials.
    fn poly_mul_asc(&self, a: &[u16], b: &[u16]) -> Vec<u16> {
        if a.is_empty() || b.is_empty() {
            return Vec::new();
        }
        let mut out = vec![0u16; a.len() + b.len() - 1];
        for (i, &x) in a.iter().enumerate() {
            if x == 0 {
                continue;
            }
            for (j, &y) in b.iter().enumerate() {
                out[i + j] = self.gf.add(out[i + j], self.gf.mul(x, y));
            }
        }
        out
    }

    /// Evaluate an ascending polynomial at `x`.
    fn eval_asc(&self, p: &[u16], x: u16) -> u16 {
        p.iter()
            .rev()
            .fold(0u16, |acc, &c| self.gf.add(self.gf.mul(acc, x), c))
    }

    /// Find the shortest linear recurrence the syndromes satisfy — the error
    /// locator polynomial, ascending, with constant term 1.
    fn berlekamp_massey(&self, syndromes: &[u16]) -> Vec<u16> {
        let mut c = vec![1u16]; // the current locator
        let mut b = vec![1u16]; // the last locator that needed correcting
        let mut b_discrepancy = 1u16; // and the discrepancy that condemned it
        let mut l = 0usize; // the current recurrence's order
        let mut shift = 1usize; // rounds since `b` was last replaced

        for n in 0..syndromes.len() {
            // The discrepancy: how far the current recurrence misses S_n.
            let mut d = syndromes[n];
            for i in 1..=l {
                if i < c.len() && n >= i {
                    d = self.gf.add(d, self.gf.mul(c[i], syndromes[n - i]));
                }
            }

            if d == 0 {
                // The recurrence already predicts this term; nothing to do.
                shift += 1;
                continue;
            }

            // c ← c − (d / b_discrepancy)·x^shift·b
            let scale = self.gf.div(d, b_discrepancy);
            let mut adjusted = c.clone();
            if adjusted.len() < b.len() + shift {
                adjusted.resize(b.len() + shift, 0);
            }
            for (i, &bi) in b.iter().enumerate() {
                adjusted[i + shift] = self.gf.add(adjusted[i + shift], self.gf.mul(scale, bi));
            }

            if 2 * l <= n {
                // The order has to grow. Keep the locator being replaced, and
                // the discrepancy that replaced it, as the correction term for
                // subsequent rounds — both unscaled, since dividing them here is
                // what makes the next round's `scale` wrong.
                let previous = std::mem::replace(&mut c, adjusted);
                l = n + 1 - l;
                b = previous;
                b_discrepancy = d;
                shift = 1;
            } else {
                c = adjusted;
                shift += 1;
            }
        }

        while c.len() > 1 && *c.last().unwrap() == 0 {
            c.pop();
        }
        c
    }

    /// Every codeword position whose locator root is present.
    fn chien_search(&self, lambda: &[u16], n: usize) -> Vec<usize> {
        (0..n)
            .filter(|&i| {
                // The root that would sit at position i, inverted, since Λ is
                // written with roots at the inverses of the locator values.
                let x_inv = self.gf.exp_of(-((n - 1 - i) as i64));
                self.eval_asc(lambda, x_inv) == 0
            })
            .collect()
    }

    /// Compute each damaged symbol's true value and write it back.
    fn forney(
        &self,
        work: &mut [u16],
        lambda: &[u16],
        omega: &[u16],
        positions: &[usize],
        n: usize,
    ) -> Result<(), RsError> {
        // Λ′ over GF(2^m): doubling annihilates, so only odd-degree terms survive.
        let derivative: Vec<u16> = lambda
            .iter()
            .enumerate()
            .skip(1)
            .map(|(i, &c)| if i % 2 == 1 { c } else { 0 })
            .collect();

        for &p in positions {
            let x = self.gf.exp_of((n - 1 - p) as i64);
            let x_inv = self.gf.inv(x);
            let denominator = self.eval_asc(&derivative, x_inv);
            if denominator == 0 {
                // A repeated root. The locator is not describing a real error
                // pattern, so there is nothing honest to write here.
                return Err(RsError::Uncorrectable);
            }
            let numerator = self.eval_asc(omega, x_inv);
            // First consecutive root is α⁰, so the usual X^(1−c) factor is X.
            let magnitude = self.gf.mul(x, self.gf.div(numerator, denominator));
            work[p] = self.gf.add(work[p], magnitude);
        }
        Ok(())
    }
}

/// `g(x) = ∏ (x − α^j)` for `j` in `0..parity`, descending, `g[0] = 1`.
fn build_generator(gf: &Gf, parity: usize) -> Vec<u16> {
    let mut g = vec![1u16];
    for j in 0..parity {
        let root = gf.exp_of(j as i64);
        let mut next = vec![0u16; g.len() + 1];
        for (i, &c) in g.iter().enumerate() {
            next[i] = gf.add(next[i], c); // ·x
            next[i + 1] = gf.add(next[i + 1], gf.mul(c, root)); // ·(−α^j) = ·α^j
        }
        g = next;
    }
    g
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A deterministic stand-in for a random source. Tests must be reproducible:
    /// a decoder bug that shows up one run in fifty is worthless if the run
    /// cannot be repeated.
    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> u64 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            self.0 >> 11
        }
        fn below(&mut self, n: usize) -> usize {
            (self.next() % n as u64) as usize
        }
        fn symbol(&mut self, gf: &Gf) -> u16 {
            (self.next() % gf.order() as u64) as u16
        }
    }

    /// The two widths the shipped wordlists actually use.
    const WIDTHS: &[u32] = &[11, 15];

    #[test]
    fn a_clean_codeword_needs_no_repair() {
        for &m in WIDTHS {
            let rs = Rs::new(m, 4).unwrap();
            let msg: Vec<u16> = (1..=20u16).collect();
            let code = rs.encode(&msg).unwrap();
            assert!(rs.is_codeword(&code), "GF(2^{m})");
            let out = rs.decode(&code, &[]).unwrap();
            assert_eq!(out.codeword, code);
            assert_eq!(out.repairs(), 0);
            assert_eq!(rs.message_of(&out.codeword), &msg[..]);
        }
    }

    #[test]
    fn encoding_is_systematic() {
        // The message must survive untouched: the words a reader already has in
        // front of them cannot be rewritten by adding parity.
        let rs = Rs::new(11, 6).unwrap();
        let msg: Vec<u16> = vec![1, 2047, 0, 1023, 5];
        let code = rs.encode(&msg).unwrap();
        assert_eq!(&code[..msg.len()], &msg[..]);
        assert_eq!(code.len(), msg.len() + 6);
    }

    #[test]
    fn errors_up_to_half_the_parity_are_corrected() {
        for &m in WIDTHS {
            for parity in [2usize, 4, 6, 8] {
                let rs = Rs::new(m, parity).unwrap();
                let gf = rs.field().clone();
                let mut rng = Lcg(0xC0FFEE ^ m as u64 ^ parity as u64);
                for trial in 0..40 {
                    let msg: Vec<u16> = (0..24).map(|_| rng.symbol(&gf)).collect();
                    let code = rs.encode(&msg).unwrap();
                    let mut damaged = code.clone();

                    let e = parity / 2;
                    let mut hit = Vec::new();
                    while hit.len() < e {
                        let p = rng.below(damaged.len());
                        if !hit.contains(&p) {
                            hit.push(p);
                        }
                    }
                    for &p in &hit {
                        // Guarantee an actual change, or the test proves nothing.
                        let mut v = rng.symbol(&gf);
                        if v == damaged[p] {
                            v = gf.add(v, 1);
                        }
                        damaged[p] = v;
                    }

                    let out = rs
                        .decode(&damaged, &[])
                        .unwrap_or_else(|e| panic!("GF(2^{m}) parity {parity} trial {trial}: {e}"));
                    assert_eq!(out.codeword, code, "GF(2^{m}) parity {parity} trial {trial}");
                    hit.sort_unstable();
                    let mut found = out.errors.clone();
                    found.sort_unstable();
                    assert_eq!(found, hit, "the located errors must be the ones we made");
                }
            }
        }
    }

    #[test]
    fn erasures_up_to_the_full_parity_are_corrected() {
        // The headline property: a located fault costs ONE parity symbol, so
        // `parity` erasures are repairable where only `parity/2` errors are.
        for &m in WIDTHS {
            for parity in [2usize, 4, 6, 8] {
                let rs = Rs::new(m, parity).unwrap();
                let gf = rs.field().clone();
                let mut rng = Lcg(0xBEEF ^ m as u64 ^ (parity as u64) << 8);
                for trial in 0..40 {
                    let msg: Vec<u16> = (0..24).map(|_| rng.symbol(&gf)).collect();
                    let code = rs.encode(&msg).unwrap();
                    let mut damaged = code.clone();

                    let mut hit = Vec::new();
                    while hit.len() < parity {
                        let p = rng.below(damaged.len());
                        if !hit.contains(&p) {
                            hit.push(p);
                        }
                    }
                    // An erasure carries no value at all; zero stands for absence.
                    for &p in &hit {
                        damaged[p] = 0;
                    }
                    hit.sort_unstable();

                    let out = rs
                        .decode(&damaged, &hit)
                        .unwrap_or_else(|e| panic!("GF(2^{m}) parity {parity} trial {trial}: {e}"));
                    assert_eq!(out.codeword, code, "GF(2^{m}) parity {parity} trial {trial}");
                    assert!(out.errors.is_empty(), "everything was located in advance");
                }
            }
        }
    }

    #[test]
    fn the_budget_is_two_errors_per_erasure_saved() {
        // 2e + f ≤ parity, exercised at the boundary for every split.
        for &m in WIDTHS {
            let parity = 8usize;
            let rs = Rs::new(m, parity).unwrap();
            let gf = rs.field().clone();
            for f in 0..=parity {
                let e = rs.correctable_errors(f);
                assert!(2 * e + f <= parity, "the budget must hold for f={f}");
                let mut rng = Lcg(0x5EED ^ m as u64 ^ (f as u64) << 16);
                for trial in 0..25 {
                    let msg: Vec<u16> = (0..30).map(|_| rng.symbol(&gf)).collect();
                    let code = rs.encode(&msg).unwrap();
                    let mut damaged = code.clone();

                    let mut chosen = Vec::new();
                    while chosen.len() < e + f {
                        let p = rng.below(damaged.len());
                        if !chosen.contains(&p) {
                            chosen.push(p);
                        }
                    }
                    let (erased, errored) = chosen.split_at(f);
                    let mut erased = erased.to_vec();
                    erased.sort_unstable();
                    for &p in &erased {
                        damaged[p] = 0;
                    }
                    for &p in errored {
                        let mut v = rng.symbol(&gf);
                        if v == damaged[p] {
                            v = gf.add(v, 1);
                        }
                        damaged[p] = v;
                    }

                    let out = rs.decode(&damaged, &erased).unwrap_or_else(|err| {
                        panic!("GF(2^{m}) f={f} e={e} trial {trial}: {err}")
                    });
                    assert_eq!(out.codeword, code, "GF(2^{m}) f={f} e={e} trial {trial}");
                }
            }
        }
    }

    #[test]
    fn damage_beyond_the_bound_fails_loudly_and_never_mis_corrects() {
        // The failure mode that matters. A decoder handing back its best effort
        // past the bound is how a burst becomes a valid-looking wrong answer, so
        // the requirement is: either refuse, or be exactly right. Never a third
        // thing.
        for &m in WIDTHS {
            let parity = 4usize;
            let rs = Rs::new(m, parity).unwrap();
            let gf = rs.field().clone();
            let mut rng = Lcg(0xDEAD ^ m as u64);
            let mut refusals = 0;
            let trials = 200;
            for _ in 0..trials {
                let msg: Vec<u16> = (0..20).map(|_| rng.symbol(&gf)).collect();
                let code = rs.encode(&msg).unwrap();
                let mut damaged = code.clone();

                // One more error than the parity can carry.
                let over = parity / 2 + 1;
                let mut hit = Vec::new();
                while hit.len() < over {
                    let p = rng.below(damaged.len());
                    if !hit.contains(&p) {
                        hit.push(p);
                    }
                }
                for &p in &hit {
                    let mut v = rng.symbol(&gf);
                    if v == damaged[p] {
                        v = gf.add(v, 1);
                    }
                    damaged[p] = v;
                }

                match rs.decode(&damaged, &[]) {
                    Err(RsError::Uncorrectable) => refusals += 1,
                    Err(e) => panic!("GF(2^{m}): unexpected error {e}"),
                    Ok(out) => assert_eq!(
                        out.codeword, code,
                        "GF(2^{m}): a decode that succeeds past the bound must still be right"
                    ),
                }
            }
            assert!(
                refusals > trials / 4,
                "GF(2^{m}): beyond-bound damage should usually be refused, got {refusals}/{trials}"
            );
        }
    }

    #[test]
    fn more_erasures_than_parity_are_refused_by_name() {
        let rs = Rs::new(11, 4).unwrap();
        let code = rs.encode(&[1, 2, 3, 4, 5]).unwrap();
        let err = rs.decode(&code, &[0, 1, 2, 3, 4]).unwrap_err();
        assert_eq!(err, RsError::TooManyErasures { erasures: 5, parity: 4 });
    }

    #[test]
    fn a_repeated_erasure_position_counts_once() {
        // Naming the same hole twice must not spend the budget twice — a caller
        // assembling positions from several passes should not be penalized.
        let rs = Rs::new(11, 2).unwrap();
        let msg: Vec<u16> = vec![7, 8, 9, 10];
        let code = rs.encode(&msg).unwrap();
        let mut damaged = code.clone();
        damaged[1] = 0;
        damaged[3] = 0;
        let out = rs.decode(&damaged, &[1, 3, 1, 3, 1]).unwrap();
        assert_eq!(out.codeword, code);
        assert_eq!(out.erasures, vec![1, 3]);
    }

    #[test]
    fn an_erasure_outside_the_codeword_is_refused() {
        let rs = Rs::new(11, 4).unwrap();
        let code = rs.encode(&[1, 2, 3]).unwrap();
        let len = code.len();
        assert_eq!(
            rs.decode(&code, &[len]).unwrap_err(),
            RsError::ErasureOutOfRange { position: len, len }
        );
    }

    #[test]
    fn a_symbol_outside_the_field_is_refused() {
        // A word index past the end of the wordlist it claims to come from.
        let rs = Rs::new(11, 2).unwrap();
        assert_eq!(
            rs.encode(&[1, 2048, 3]).unwrap_err(),
            RsError::SymbolOutOfField { symbol: 2048, order: 2048 }
        );
    }

    #[test]
    fn a_codeword_longer_than_the_field_is_refused() {
        let rs = Rs::new(11, 2).unwrap();
        let msg = vec![1u16; 2048];
        assert_eq!(
            rs.encode(&msg).unwrap_err(),
            RsError::TooLong { len: 2050, max: 2047 }
        );
    }

    #[test]
    fn an_erased_symbol_whose_true_value_was_zero_is_still_recovered() {
        // The awkward case: zero is both "no symbol" and a legitimate symbol.
        // Erasing a slot that genuinely held zero leaves the syndromes clean,
        // and the decoder must report that slot as an erasure it filled rather
        // than claim there was nothing to do.
        let rs = Rs::new(11, 4).unwrap();
        let msg: Vec<u16> = vec![5, 0, 9, 0, 2];
        let code = rs.encode(&msg).unwrap();
        let out = rs.decode(&code, &[1, 3]).unwrap();
        assert_eq!(out.codeword, code);
        assert_eq!(rs.message_of(&out.codeword), &msg[..]);
    }

    #[test]
    fn parity_cost_is_one_word_per_symbol() {
        // The accounting #81 rests on, in the units the prose is measured in.
        // A 32-byte payload is 24 words in GF(2¹¹); correcting one located
        // fault costs one more word, one unlocated fault costs two.
        let rs1 = Rs::new(11, 1).unwrap();
        let rs2 = Rs::new(11, 2).unwrap();
        let msg: Vec<u16> = (0..24u16).collect();
        assert_eq!(rs1.encode(&msg).unwrap().len(), 25);
        assert_eq!(rs2.encode(&msg).unwrap().len(), 26);
        assert_eq!(rs1.correctable_errors(0), 0, "one parity symbol cannot locate");
        assert_eq!(rs1.correctable_errors(1), 0, "but it can fill one hole");
        assert_eq!(rs2.correctable_errors(0), 1);
    }

    #[test]
    fn one_parity_symbol_repairs_one_erasure() {
        let rs = Rs::new(11, 1).unwrap();
        let msg: Vec<u16> = (100..124u16).collect();
        let code = rs.encode(&msg).unwrap();
        for p in 0..code.len() {
            let mut damaged = code.clone();
            damaged[p] = 0;
            let out = rs.decode(&damaged, &[p]).unwrap();
            assert_eq!(out.codeword, code, "erasure at {p}");
        }
    }

    #[test]
    fn latin_sized_payloads_work_in_the_larger_field() {
        // Latin's wordlist is 2¹⁵, so its symbols carry 15 bits and its
        // codewords may run to 32767 — far past anything the book needs, but
        // the arithmetic has to be right at that width too.
        let rs = Rs::for_wordlist_len(32768, 4).unwrap();
        assert_eq!(rs.field().m(), 15);
        let msg: Vec<u16> = vec![32767, 0, 12345, 1, 30000];
        let code = rs.encode(&msg).unwrap();
        let mut damaged = code.clone();
        damaged[2] = 9;
        damaged[4] = 0;
        let out = rs.decode(&damaged, &[4]).unwrap();
        assert_eq!(out.codeword, code);
        assert_eq!(out.errors, vec![2]);
        assert_eq!(out.erasures, vec![4]);
    }

    #[test]
    fn a_non_power_of_two_wordlist_has_no_code() {
        assert_eq!(
            Rs::for_wordlist_len(2047, 4).unwrap_err(),
            RsError::Field(GfError::NotAPowerOfTwo(2047))
        );
    }
}
