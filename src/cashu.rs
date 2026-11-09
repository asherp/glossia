//! Cashu tokens as prose, in a compact envelope.
//!
//! A Cashu V4 (`cashuB…`) token is packed into a compact envelope and then
//! written with [`canonical_encode`](crate::canonical_encode), so it gets the
//! canonical form's checksum and Reed–Solomon parity: a few mistyped words
//! are repaired on the way back. The envelope roughly halves the token. A V4
//! token stores each 32-byte secret as 64 hex characters; the envelope
//! stores the raw 32 bytes. A 2-proof token is about 250 English words
//! instead of about 400.
//!
//! Envelope (all lengths one byte), shared with Hashu's `hashu_cli::words`:
//!
//! ```text
//! 0x00 cbor..                       a V4 token's CBOR, unchanged (fallback)
//! 0x01 flags len url len unit [len memo]
//!      nkeysets { len id  nproofs { exp secret[32] C[33] } }
//! ```
//!
//! `flags` bit 0: the URL is written without its `https://`; bit 1: a
//! memo follows. The mint URL and unit are always written: the format
//! assumes no mint and no unit. A proof's amount is `2^exp`. DLEQ
//! proofs are left out (the receiver checks the token with its mint
//! instead). A token the envelope can't hold exactly (a NUT-10 locked
//! secret, a witness, an amount that isn't a power of two) is carried as
//! `0x00` raw CBOR, so nothing but DLEQ is ever dropped.

use ciborium::value::Value;

use crate::canonical::{canonical_decode, canonical_encode, CanonicalError};
use crate::codec::{base64_decode, base64_encode};

const RAW: u8 = 0x00;
const COMPACT: u8 = 0x01;
const HTTPS_IMPLIED: u8 = 1;
const HAS_MEMO: u8 = 2;

/// Why a token could not be written or read as prose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CashuError {
    /// The input is not a `cashuB…` token.
    NotV4Token,
    /// The prose decoded, but not to a Cashu envelope.
    NotCashu,
    Canonical(CanonicalError),
}

impl std::fmt::Display for CashuError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotV4Token => write!(f, "not a cashuB token"),
            Self::NotCashu => write!(f, "the text decodes, but not to a Cashu token"),
            Self::Canonical(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for CashuError {}

/// A token read back from prose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CashuDecoded {
    /// The `cashuB…` token.
    pub token: String,
    /// Word positions parity repaired (empty when the text arrived intact).
    pub repaired: Vec<usize>,
}

/// `cashuB…` → canonical prose in `language`/`wordlist`.
pub fn cashu_to_words(token: &str, language: &str, wordlist: &str) -> Result<String, CashuError> {
    let cbor = token_cbor(token.trim()).ok_or(CashuError::NotV4Token)?;
    canonical_encode(&pack(&cbor), language, wordlist).map_err(CashuError::Canonical)
}

/// Canonical prose → `cashuB…`. Text around the paragraph (a greeting, a
/// sign-off) is dropped by trying whole sentences off each end, since
/// ordinary words such as "about" or "you" are payload words too.
pub fn cashu_from_words(text: &str, language: &str, wordlist: &str) -> Result<CashuDecoded, CashuError> {
    let cuts = sentence_starts(text);
    const TRIM: usize = 4;
    let mut first_err = None;
    for &a in cuts.iter().take(TRIM + 1) {
        let ends = std::iter::once(text.len()).chain(cuts.iter().rev().copied().take(TRIM));
        for b in ends {
            if b <= a {
                continue;
            }
            match canonical_decode(&text[a..b], language, wordlist) {
                Ok(d) => {
                    if let Some(token) = unpack(&d.payload).and_then(|c| cbor_token(&c)) {
                        return Ok(CashuDecoded { token, repaired: d.repaired });
                    }
                    first_err.get_or_insert(CashuError::NotCashu);
                }
                Err(e) => {
                    first_err.get_or_insert(CashuError::Canonical(e));
                }
            }
        }
    }
    Err(first_err.unwrap_or(CashuError::NotCashu))
}

/// Byte offsets where a sentence or line starts, the first being 0.
fn sentence_starts(text: &str) -> Vec<usize> {
    let mut out = vec![0];
    let mut after_stop = false;
    for (i, ch) in text.char_indices() {
        if matches!(ch, '.' | '!' | '?' | ':' | '\n') {
            after_stop = true;
        } else if after_stop && !ch.is_whitespace() {
            out.push(i);
            after_stop = false;
        }
    }
    out
}

/// The CBOR inside a `cashuB…` token (base64url, padding optional; plain
/// base64 is accepted too).
fn token_cbor(token: &str) -> Option<Vec<u8>> {
    let body = token.strip_prefix("cashuB")?;
    let mut b64: String = body
        .chars()
        .map(|c| match c {
            '-' => '+',
            '_' => '/',
            c => c,
        })
        .filter(|&c| c != '=')
        .collect();
    while b64.len() % 4 != 0 {
        b64.push('=');
    }
    base64_decode(&b64)
}

/// CBOR → `cashuB…` (base64url, unpadded), if it has a V4 token's shape.
fn cbor_token(cbor: &[u8]) -> Option<String> {
    let v: Value = ciborium::de::from_reader(cbor).ok()?;
    let top = v.as_map()?;
    get(top, "m")?.as_text()?;
    get(top, "u")?.as_text()?;
    get(top, "t")?.as_array()?;
    let b64: String = base64_encode(cbor)
        .chars()
        .filter(|&c| c != '=')
        .map(|c| match c {
            '+' => '-',
            '/' => '_',
            c => c,
        })
        .collect();
    Some(format!("cashuB{b64}"))
}

fn pack(cbor: &[u8]) -> Vec<u8> {
    compact(cbor).unwrap_or_else(|| {
        let mut out = vec![RAW];
        out.extend_from_slice(cbor);
        out
    })
}

fn unpack(packed: &[u8]) -> Option<Vec<u8>> {
    match *packed.first()? {
        RAW => Some(packed[1..].to_vec()),
        COMPACT => expand(&packed[1..]),
        _ => None,
    }
}

fn get<'a>(map: &'a [(Value, Value)], key: &str) -> Option<&'a Value> {
    map.iter()
        .find(|(k, _)| k.as_text() == Some(key))
        .map(|(_, v)| v)
}

fn push_short(out: &mut Vec<u8>, bytes: &[u8]) -> Option<()> {
    out.push(u8::try_from(bytes.len()).ok()?);
    out.extend_from_slice(bytes);
    Some(())
}

/// A V4 token's CBOR → the compact envelope, or `None` if it would lose
/// anything other than DLEQ proofs.
fn compact(cbor: &[u8]) -> Option<Vec<u8>> {
    let v: Value = ciborium::de::from_reader(cbor).ok()?;
    let top = v.as_map()?;
    if top
        .iter()
        .any(|(k, _)| !matches!(k.as_text(), Some("m" | "u" | "d" | "t")))
    {
        return None;
    }
    let mint = get(top, "m")?.as_text()?;
    let unit = get(top, "u")?.as_text()?;
    let memo = match get(top, "d") {
        None => None,
        Some(d) => Some(d.as_text()?),
    };
    let mut flags = 0u8;
    let mut head = Vec::new();
    // The mint and unit are always written: the format favours no mint
    // and no unit.
    match mint.strip_prefix("https://") {
        Some(rest) => {
            flags |= HTTPS_IMPLIED;
            push_short(&mut head, rest.as_bytes())?;
        }
        None => push_short(&mut head, mint.as_bytes())?,
    }
    push_short(&mut head, unit.as_bytes())?;
    if let Some(memo) = memo {
        flags |= HAS_MEMO;
        push_short(&mut head, memo.as_bytes())?;
    }
    let mut out = vec![COMPACT, flags];
    out.extend(head);
    let keysets = get(top, "t")?.as_array()?;
    out.push(u8::try_from(keysets.len()).ok()?);
    for ks in keysets {
        let ks = ks.as_map()?;
        if ks
            .iter()
            .any(|(k, _)| !matches!(k.as_text(), Some("i" | "p")))
        {
            return None;
        }
        push_short(&mut out, get(ks, "i")?.as_bytes()?)?;
        let proofs = get(ks, "p")?.as_array()?;
        out.push(u8::try_from(proofs.len()).ok()?);
        for p in proofs {
            let p = p.as_map()?;
            if p.iter()
                .any(|(k, _)| !matches!(k.as_text(), Some("a" | "s" | "c" | "d")))
            {
                return None;
            }
            let a = u64::try_from(get(p, "a")?.as_integer()?).ok()?;
            if !a.is_power_of_two() {
                return None;
            }
            out.push(a.trailing_zeros() as u8);
            let s = get(p, "s")?.as_text()?;
            // Only cdk's default secret, 64 lowercase hex, round-trips.
            if s.len() != 64 || !s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
                return None;
            }
            out.extend(crate::codec::hex_decode(s)?);
            let c = get(p, "c")?.as_bytes()?;
            if c.len() != 33 {
                return None;
            }
            out.extend_from_slice(c);
        }
    }
    Some(out)
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        if self.0.len() < n {
            return None;
        }
        let (a, b) = self.0.split_at(n);
        self.0 = b;
        Some(a)
    }
    fn byte(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }
    fn short(&mut self) -> Option<&'a [u8]> {
        let n = self.byte()? as usize;
        self.take(n)
    }
    fn text(&mut self) -> Option<String> {
        String::from_utf8(self.short()?.to_vec()).ok()
    }
}

/// The compact envelope (after its tag byte) → a V4 token's CBOR.
fn expand(body: &[u8]) -> Option<Vec<u8>> {
    let mut r = Reader(body);
    let flags = r.byte()?;
    if flags & !(HTTPS_IMPLIED | HAS_MEMO) != 0 {
        return None;
    }
    let m = r.text()?;
    let mint = if flags & HTTPS_IMPLIED != 0 {
        format!("https://{m}")
    } else {
        m
    };
    let unit = r.text()?;
    let memo = if flags & HAS_MEMO != 0 {
        Some(r.text()?)
    } else {
        None
    };
    let t = |s: &str| Value::Text(s.to_string());
    let mut keysets = Vec::new();
    for _ in 0..r.byte()? {
        let id = r.short()?.to_vec();
        let mut proofs = Vec::new();
        for _ in 0..r.byte()? {
            let exp = r.byte()?;
            if exp > 63 {
                return None;
            }
            let secret = crate::codec::hex_encode(r.take(32)?);
            let c = r.take(33)?.to_vec();
            proofs.push(Value::Map(vec![
                (t("a"), Value::Integer((1u64 << exp).into())),
                (t("s"), Value::Text(secret)),
                (t("c"), Value::Bytes(c)),
            ]));
        }
        keysets.push(Value::Map(vec![
            (t("i"), Value::Bytes(id)),
            (t("p"), Value::Array(proofs)),
        ]));
    }
    if !r.0.is_empty() {
        return None;
    }
    let mut top = vec![(t("m"), Value::Text(mint)), (t("u"), Value::Text(unit))];
    if let Some(memo) = memo {
        top.push((t("d"), Value::Text(memo)));
    }
    top.push((t("t"), Value::Array(keysets)));
    let mut out = Vec::new();
    ciborium::ser::into_writer(&Value::Map(top), &mut out).ok()?;
    Some(out)
}


#[cfg(test)]
mod tests {
    use super::*;

    /// A 2-proof THH token from Hashu's test mint, and its canonical English.
    /// Hashu encodes with its own copy of the envelope, so this pins the two
    /// implementations to each other.
    const VECTOR_TOKEN: &str = include_str!("../tests/cashu_vector_token.txt");
    const VECTOR_WORDS: &str = include_str!("../tests/cashu_vector_words.txt");

    #[test]
    fn hashu_vector_round_trips() {
        let words = cashu_to_words(VECTOR_TOKEN.trim(), "english", "bip39").unwrap();
        assert_eq!(words, VECTOR_WORDS.trim());
        let back = cashu_from_words(VECTOR_WORDS, "english", "bip39").unwrap();
        assert!(back.repaired.is_empty());
        // The vector carries DLEQ proofs, which the envelope leaves out: the
        // token read back is the same proofs without them, and re-encodes
        // to the same words.
        assert!(back.token.len() < VECTOR_TOKEN.trim().len());
        assert_eq!(cashu_to_words(&back.token, "english", "bip39").unwrap(), words);
    }

    #[test]
    fn compact_shrinks_the_token() {
        let cbor = token_cbor(VECTOR_TOKEN.trim()).unwrap();
        let packed = pack(&cbor);
        assert_eq!(packed[0], COMPACT);
        assert!(packed.len() * 100 < cbor.len() * 70, "{} vs {}", packed.len(), cbor.len());
    }

    #[test]
    fn greeting_and_sign_off_are_ignored() {
        let msg = format!("Hey, this is about the hashrate I promised you:\n\n{}\n\nThanks, see you soon!", VECTOR_WORDS.trim());
        let plain = cashu_from_words(VECTOR_WORDS, "english", "bip39").unwrap().token;
        assert_eq!(cashu_from_words(&msg, "english", "bip39").unwrap().token, plain);
    }

    #[test]
    fn locked_secrets_fall_back_to_raw_cbor() {
        // A NUT-10 P2PK secret is JSON, not 64 hex characters.
        let t = |s: &str| Value::Text(s.to_string());
        let proof = Value::Map(vec![
            (t("a"), Value::Integer(4.into())),
            (t("s"), t(r#"["P2PK",{"nonce":"00","data":"02aa"}]"#)),
            (t("c"), Value::Bytes(vec![2; 33])),
        ]);
        let tok = Value::Map(vec![
            (t("m"), t("https://mint.example")),
            (t("u"), t("sat")),
            (t("t"), Value::Array(vec![Value::Map(vec![
                (t("i"), Value::Bytes(vec![0; 8])),
                (t("p"), Value::Array(vec![proof])),
            ])])),
        ]);
        let mut cbor = Vec::new();
        ciborium::ser::into_writer(&tok, &mut cbor).unwrap();
        assert_eq!(pack(&cbor)[0], RAW);
        let token = cbor_token(&cbor).unwrap();
        let words = cashu_to_words(&token, "english", "bip39").unwrap();
        assert_eq!(cashu_from_words(&words, "english", "bip39").unwrap().token, token);
    }

    #[test]
    fn plain_prose_is_not_a_token() {
        assert!(cashu_from_words("The quick brown fox jumps over the lazy dog.", "english", "bip39").is_err());
        assert_eq!(cashu_to_words("cashuAeyJ0b2tlbiI6W119", "english", "bip39"), Err(CashuError::NotV4Token));
    }
}
