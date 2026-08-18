//! Arithmetic in GF(2^m), the field a payload word lives in.
//!
//! A payload word carries `m` bits and its index is an integer in `0..2^m`. Take
//! that index as a field element and one mistranscribed word is exactly one
//! wrong symbol — which is the whole reason to work here rather than over bytes.
//! An 11-bit word straddles two or three byte boundaries, so a byte-oriented
//! code has to spend two or three symbols' worth of parity to repair what is
//! really a single fault.
//!
//! Every shipped payload wordlist is a power of two, so every one of them has a
//! field: 2¹¹ for english, czech and german, 2¹⁵ for latin. `m` is therefore a
//! property of the wordlist rather than a constant, and this module is generic
//! over it instead of pinning one width.
//!
//! Elements are `u16`, which carries m ≤ 16. Addition is XOR — the field has
//! characteristic 2, so addition and subtraction are the same operation, and
//! negation is the identity. Multiplication goes through log/antilog tables
//! built once per width and shared process-wide.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

/// Primitive polynomials, indexed by degree, as bit patterns with the `x^m` term
/// included. Each is primitive rather than merely irreducible: the field is
/// built by walking the powers of `x`, which enumerates every nonzero element
/// exactly once only if `x` generates the whole multiplicative group.
///
/// [`Gf::new`] proves this rather than trusting it — it checks that the walk
/// visits `2^m - 1` distinct elements before returning to 1.
const PRIMITIVE: [u32; 17] = [
    0, 0, // m = 0, 1: no field here
    0x7,     // m=2:  x² + x + 1
    0xB,     // m=3:  x³ + x + 1
    0x13,    // m=4:  x⁴ + x + 1
    0x25,    // m=5:  x⁵ + x² + 1
    0x43,    // m=6:  x⁶ + x + 1
    0x89,    // m=7:  x⁷ + x³ + 1
    0x11D,   // m=8:  x⁸ + x⁴ + x³ + x² + 1
    0x211,   // m=9:  x⁹ + x⁴ + 1
    0x409,   // m=10: x¹⁰ + x³ + 1
    0x805,   // m=11: x¹¹ + x² + 1        — english, czech, german
    0x1053,  // m=12: x¹² + x⁶ + x⁴ + x + 1
    0x201B,  // m=13: x¹³ + x⁴ + x³ + x + 1
    0x4443,  // m=14: x¹⁴ + x¹⁰ + x⁶ + x + 1
    0x8003,  // m=15: x¹⁵ + x + 1         — latin
    0x1100B, // m=16: x¹⁶ + x¹² + x³ + x + 1
];

/// The widths this module can build a field for.
pub const MIN_M: u32 = 2;
pub const MAX_M: u32 = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GfError {
    /// `m` is outside [`MIN_M`]..=[`MAX_M`].
    UnsupportedWidth(u32),
    /// The wordlist length is not a power of two, so its indices are not field
    /// elements and no amount of parity can be defined over them.
    NotAPowerOfTwo(usize),
}

impl std::fmt::Display for GfError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GfError::UnsupportedWidth(m) => write!(
                f,
                "GF(2^{m}) is outside the supported range {MIN_M}..={MAX_M}"
            ),
            GfError::NotAPowerOfTwo(n) => write!(
                f,
                "a wordlist of {n} words has no field: word indices are field elements only when the count is a power of two"
            ),
        }
    }
}

impl std::error::Error for GfError {}

/// A binary extension field GF(2^m).
#[derive(Debug)]
pub struct Gf {
    m: u32,
    /// `2^m` — one more than the largest element.
    order: usize,
    /// `exp[i] = α^i`, laid out to twice the group order so that a product of
    /// two logarithms can index it without a modulo.
    exp: Vec<u16>,
    /// `log[α^i] = i`. `log[0]` is meaningless and never read: zero has no
    /// logarithm, and every entry point guards it.
    log: Vec<u16>,
}

impl Gf {
    /// Build GF(2^m).
    ///
    /// Prefer [`Gf::cached`] on any path that runs more than once — the tables
    /// for m=15 are 32767 entries and there is no reason to build them twice.
    pub fn new(m: u32) -> Result<Self, GfError> {
        if !(MIN_M..=MAX_M).contains(&m) {
            return Err(GfError::UnsupportedWidth(m));
        }
        let order = 1usize << m;
        let group = order - 1; // the multiplicative group's size
        let poly = PRIMITIVE[m as usize];

        let mut exp = vec![0u16; group * 2];
        let mut log = vec![0u16; order];

        // Walk the powers of x. Each step shifts left — multiplying by x — and
        // reduces modulo the primitive polynomial when the degree reaches m.
        let mut a: u32 = 1;
        for i in 0..group {
            exp[i] = a as u16;
            log[a as usize] = i as u16;
            a <<= 1;
            if a & (1 << m) != 0 {
                a ^= poly;
            }
        }
        debug_assert_eq!(a, 1, "the walk must close the cycle");

        // The second copy lets `mul` index by `log(a) + log(b)` directly, which
        // cannot exceed `2 * (group - 1)`.
        for i in 0..group {
            exp[group + i] = exp[i];
        }

        let gf = Gf { m, order, exp, log };
        gf.assert_primitive()?;
        Ok(gf)
    }

    /// Confirm the tabled polynomial really is primitive for this width. A
    /// merely irreducible polynomial would give a valid field whose `x` is not a
    /// generator, so the walk would revisit elements and the log table would be
    /// silently wrong — the kind of fault that produces plausible garbage rather
    /// than an error.
    fn assert_primitive(&self) -> Result<(), GfError> {
        let group = self.order - 1;
        let mut seen = vec![false; self.order];
        for i in 0..group {
            let v = self.exp[i] as usize;
            if v == 0 || seen[v] {
                return Err(GfError::UnsupportedWidth(self.m));
            }
            seen[v] = true;
        }
        Ok(())
    }

    /// The shared field for this width, built once per process.
    pub fn cached(m: u32) -> Result<Arc<Gf>, GfError> {
        static FIELDS: OnceLock<Mutex<HashMap<u32, Arc<Gf>>>> = OnceLock::new();
        let map = FIELDS.get_or_init(|| Mutex::new(HashMap::new()));
        if let Some(f) = map.lock().unwrap().get(&m) {
            return Ok(f.clone());
        }
        // Built outside the lock so a failure cannot poison the cache for every
        // later caller, matching how the wordlist caches are built.
        let field = Arc::new(Gf::new(m)?);
        map.lock().unwrap().insert(m, field.clone());
        Ok(field)
    }

    /// The field whose elements are the indices of a wordlist of `len` words.
    pub fn for_wordlist_len(len: usize) -> Result<Arc<Gf>, GfError> {
        if len < 2 || !len.is_power_of_two() {
            return Err(GfError::NotAPowerOfTwo(len));
        }
        Gf::cached(len.trailing_zeros())
    }

    /// Bits per symbol.
    pub fn m(&self) -> u32 {
        self.m
    }

    /// `2^m`: the element count, and one past the largest valid element.
    pub fn order(&self) -> usize {
        self.order
    }

    /// The multiplicative group's size, `2^m - 1`. This is also the longest
    /// codeword the field admits, which is what bounds a payload's word count.
    pub fn group_order(&self) -> usize {
        self.order - 1
    }

    /// Addition, which in characteristic 2 is also subtraction.
    #[inline]
    pub fn add(&self, a: u16, b: u16) -> u16 {
        a ^ b
    }

    #[inline]
    pub fn mul(&self, a: u16, b: u16) -> u16 {
        if a == 0 || b == 0 {
            return 0;
        }
        let i = self.log[a as usize] as usize + self.log[b as usize] as usize;
        self.exp[i]
    }

    /// `a / b`. Panics on division by zero, which is a caller bug rather than a
    /// data condition: no decoding step divides by a value it has not already
    /// established is nonzero.
    #[inline]
    pub fn div(&self, a: u16, b: u16) -> u16 {
        assert!(b != 0, "division by zero in GF(2^{})", self.m);
        if a == 0 {
            return 0;
        }
        let group = self.group_order();
        let i = self.log[a as usize] as usize + group - self.log[b as usize] as usize;
        self.exp[i % group]
    }

    #[inline]
    pub fn inv(&self, a: u16) -> u16 {
        assert!(a != 0, "zero has no inverse in GF(2^{})", self.m);
        let group = self.group_order();
        self.exp[(group - self.log[a as usize] as usize) % group]
    }

    /// `α^i`, for any integer exponent including negative ones.
    #[inline]
    pub fn exp_of(&self, i: i64) -> u16 {
        let group = self.group_order() as i64;
        self.exp[i.rem_euclid(group) as usize]
    }

    /// `log_α(a)`. Panics on zero, which has no logarithm.
    #[inline]
    pub fn log_of(&self, a: u16) -> usize {
        assert!(a != 0, "zero has no logarithm in GF(2^{})", self.m);
        self.log[a as usize] as usize
    }

    /// `a^n` by repeated squaring.
    pub fn pow(&self, a: u16, n: i64) -> u16 {
        if a == 0 {
            return if n == 0 { 1 } else { 0 };
        }
        let group = self.group_order() as i64;
        let e = (self.log[a as usize] as i64 * n).rem_euclid(group);
        self.exp[e as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The widths the shipped wordlists actually need, plus the extremes.
    const WIDTHS: &[u32] = &[2, 4, 8, 11, 15, 16];

    #[test]
    fn every_tabled_polynomial_is_primitive() {
        // Not decoration: an irreducible-but-not-primitive polynomial yields a
        // field whose tables are wrong in a way that still computes.
        for m in MIN_M..=MAX_M {
            let gf = Gf::new(m).unwrap_or_else(|e| panic!("GF(2^{m}): {e}"));
            let group = gf.group_order();
            let mut seen = vec![false; gf.order()];
            for i in 0..group {
                let v = gf.exp_of(i as i64);
                assert_ne!(v, 0, "GF(2^{m}): α^{i} must be nonzero");
                assert!(!seen[v as usize], "GF(2^{m}): α^{i} repeats before the cycle closes");
                seen[v as usize] = true;
            }
            assert_eq!(gf.exp_of(group as i64), 1, "GF(2^{m}): α^(2^m-1) must be 1");
        }
    }

    #[test]
    fn the_shipped_wordlists_all_have_a_field() {
        // english, czech, german are 2048; latin is 32768.
        assert_eq!(Gf::for_wordlist_len(2048).unwrap().m(), 11);
        assert_eq!(Gf::for_wordlist_len(32768).unwrap().m(), 15);
    }

    #[test]
    fn a_non_power_of_two_wordlist_is_refused_by_name() {
        // Declining beats guessing: there is no field over 2047 indices, and a
        // caller must hear that rather than get a silently truncated one.
        assert_eq!(
            Gf::for_wordlist_len(2047).err(),
            Some(GfError::NotAPowerOfTwo(2047))
        );
        assert_eq!(Gf::for_wordlist_len(0).err(), Some(GfError::NotAPowerOfTwo(0)));
        assert_eq!(Gf::for_wordlist_len(1).err(), Some(GfError::NotAPowerOfTwo(1)));
    }

    #[test]
    fn unsupported_widths_are_refused() {
        assert_eq!(Gf::new(1).err(), Some(GfError::UnsupportedWidth(1)));
        assert_eq!(Gf::new(17).err(), Some(GfError::UnsupportedWidth(17)));
    }

    #[test]
    fn addition_is_xor_and_is_its_own_inverse() {
        for &m in WIDTHS {
            let gf = Gf::new(m).unwrap();
            for a in [0usize, 1, 5, 100] {
                for b in [0usize, 1, 7, 255] {
                    // Reduce in usize: at m=16 the order is 65536 and does not
                    // fit the element type, though every element does.
                    let (a, b) = ((a % gf.order()) as u16, (b % gf.order()) as u16);
                    assert_eq!(gf.add(a, b), a ^ b);
                    assert_eq!(gf.add(gf.add(a, b), b), a, "GF(2^{m}): adding twice undoes");
                }
            }
        }
    }

    #[test]
    fn multiplication_has_an_identity_and_an_absorbing_zero() {
        for &m in WIDTHS {
            let gf = Gf::new(m).unwrap();
            for a in (0..gf.order()).step_by(7.max(gf.order() / 512)) {
                let a = a as u16;
                assert_eq!(gf.mul(a, 1), a, "GF(2^{m})");
                assert_eq!(gf.mul(a, 0), 0, "GF(2^{m})");
            }
        }
    }

    #[test]
    fn multiplication_is_commutative_and_associative() {
        for &m in WIDTHS {
            let gf = Gf::new(m).unwrap();
            let step = (gf.order() / 64).max(1);
            let vals: Vec<u16> = (0..gf.order()).step_by(step).map(|v| v as u16).collect();
            for &a in vals.iter().take(20) {
                for &b in vals.iter().take(20) {
                    assert_eq!(gf.mul(a, b), gf.mul(b, a), "GF(2^{m}) commutative");
                    for &c in vals.iter().take(8) {
                        assert_eq!(
                            gf.mul(gf.mul(a, b), c),
                            gf.mul(a, gf.mul(b, c)),
                            "GF(2^{m}) associative"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn multiplication_distributes_over_addition() {
        for &m in WIDTHS {
            let gf = Gf::new(m).unwrap();
            let step = (gf.order() / 32).max(1);
            let vals: Vec<u16> = (0..gf.order()).step_by(step).map(|v| v as u16).collect();
            for &a in vals.iter().take(16) {
                for &b in vals.iter().take(16) {
                    for &c in vals.iter().take(16) {
                        assert_eq!(
                            gf.mul(a, gf.add(b, c)),
                            gf.add(gf.mul(a, b), gf.mul(a, c)),
                            "GF(2^{m}) distributive"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn division_and_inversion_undo_multiplication() {
        for &m in WIDTHS {
            let gf = Gf::new(m).unwrap();
            let step = (gf.order() / 128).max(1);
            for a in (0..gf.order()).step_by(step) {
                for b in (1..gf.order()).step_by(step.max(1)) {
                    let (a, b) = (a as u16, b as u16);
                    assert_eq!(gf.div(gf.mul(a, b), b), a, "GF(2^{m}): div undoes mul");
                    assert_eq!(gf.mul(b, gf.inv(b)), 1, "GF(2^{m}): b · b⁻¹ = 1");
                }
            }
        }
    }

    #[test]
    fn exponent_arithmetic_wraps_at_the_group_order() {
        for &m in WIDTHS {
            let gf = Gf::new(m).unwrap();
            let group = gf.group_order() as i64;
            assert_eq!(gf.exp_of(0), 1, "GF(2^{m})");
            assert_eq!(gf.exp_of(group), 1, "GF(2^{m}): the cycle closes");
            assert_eq!(gf.exp_of(-1), gf.exp_of(group - 1), "GF(2^{m}): negative wraps");
            assert_eq!(gf.mul(gf.exp_of(1), gf.exp_of(-1)), 1, "GF(2^{m})");
        }
    }

    #[test]
    fn log_and_exp_are_inverse() {
        for &m in WIDTHS {
            let gf = Gf::new(m).unwrap();
            let step = (gf.order() / 256).max(1);
            for a in (1..gf.order()).step_by(step) {
                let a = a as u16;
                assert_eq!(gf.exp_of(gf.log_of(a) as i64), a, "GF(2^{m})");
            }
        }
    }

    #[test]
    fn pow_agrees_with_repeated_multiplication() {
        for &m in WIDTHS {
            let gf = Gf::new(m).unwrap();
            for a in [1usize, 2, 3, 17, 100] {
                let a = (a % (gf.order() - 1) + 1) as u16;
                let mut acc = 1u16;
                for n in 0..12i64 {
                    assert_eq!(gf.pow(a, n), acc, "GF(2^{m}): {a}^{n}");
                    acc = gf.mul(acc, a);
                }
                assert_eq!(gf.pow(a, -1), gf.inv(a), "GF(2^{m}): a^-1 = inv(a)");
            }
        }
    }

    #[test]
    fn the_cache_hands_back_one_field() {
        let a = Gf::cached(11).unwrap();
        let b = Gf::cached(11).unwrap();
        assert!(Arc::ptr_eq(&a, &b), "the tables must be built once");
    }
}
