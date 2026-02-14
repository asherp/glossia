# Dialect Calculus

Glossia's type system and CCG combinators don't just compose words into sentences — they compose *dialects into pipelines*. This chapter develops the **dialect calculus**: a typed lambda calculus where dialects are types and encoding operations are terms.

## Motivation

Glossia already supports translation between languages (see [How It Works](./how-it-works.md#glossia-translation)). The bitstream is the common intermediate representation:

```
Source Language Text → Binary Data (bits) → Target Language Text
```

But a "language" is actually three independent choices fused together:

| Axis | Examples | What it determines |
|------|----------|--------------------|
| **Input format** | hex (Nostr), base64 (PGP), raw bytes (BIP39) | How bits are packed into wordlist indices |
| **Vocabulary** | `payload.yaml`, `payload_base58.yaml` | Which words carry data (and bits per word) |
| **Output grammar** | `latin/body`, `english/prose`, `cs/sig` | Syntactic shape of the output |

Current dialects like `cs/sig_nostr` fuse all three: hex input, base16 vocabulary, and ASCII armor grammar. But you should be able to mix freely:

- Encode a **Nostr signature** (hex) as **Latin prose**
- Convert a **PGP message** from ASCII-armored base64 into **English body text**
- Transcode **Latin prose** into **English prose** directly

The dialect calculus makes this composition type-safe.

## Dialects as types

Recall Glossia's Montague Grammar type system:

- `e` — Entity type
- `t` — Truth type (a valid sentence / proposition)
- `A -> B` — Function type
- `Refined(r, A)` — Refined type (type `A` with refinement tag `r`)

At the **object level**, `e` is an individual thing and `t` is a sentence about things. At the **dialect level**, we reinterpret:

- `e` — a data representation (hex string, base64 string, Latin text, English text, raw bytes)
- `t` — a valid encoded output (data successfully encoded in some dialect)

The `Refined` type distinguishes dialect entities:

```
Refined("latin/body", e)      -- Latin prose as data representation
Refined("english/body", e)    -- English prose as data representation
Refined("hex", e)             -- hex string
Refined("base64", e)          -- base64 string
Refined("nostr/sig", e)       -- Nostr Schnorr signature (refines hex)
Refined("pgp/msg", e)         -- PGP message (refines base64)
Refined("bits", e)            -- raw bitstream (universal intermediate)
```

## POS tags as dialect operations

Every POS tag already has a Montague type. The dialect calculus reinterprets each type as an operation on data representations:

| POS | Type | Object-level meaning | Dialect-level meaning |
|-----|------|---------------------|----------------------|
| **N** | `e -> t` | predicate over entities | **Encoder**: given a data representation, produce valid encoded text |
| **V** | `e -> (e -> t)` | relation between entities | **Transcoder**: given a source, return an encoder for a target |
| **Adj** | `e -> e` | entity modifier | **Format transform**: rewrite one representation as another (e.g., hex decode to bytes) |
| **Det** | `(e -> t) -> (e -> t)` | quantifier | **Codec modifier**: wrap an encoder with a DataMode (ascii7, hex, base64) |
| **Adv** | `(e -> t) -> (e -> t)` | predicate modifier | **Pipeline modifier**: wrap an encoder with logging, verification, etc. |
| **Conj** | `t -> (t -> t)` | proposition connective | **Combiner**: concatenate or interleave two encoded outputs |
| **Prep** | `e -> (e -> t)` | prepositional relation | **Parameterized encoder**: "encode X *into* Y", "decode X *from* Y" |
| **Cop** | `(e -> t) -> (e -> t)` | subject-predicate linker | **Identity wrapper**: "X *is* encoded as Y" |
| **Dot** | `t` | sentence truth value | **Pipeline terminator**: the complete, validated output |

## CCG combinators as pipeline algebra

The five CCG combinators from `lambda_terms.rs` have direct dialect-level meanings:

### B — Composition (transcode)

```
B f g x = f(g(x))
```

The B combinator is the fundamental transcoding operation. Given:

- `decode_latin : Refined("latin/body", e) -> Refined("bits", e)` — an `Adj` (format transform)
- `encode_english : Refined("bits", e) -> t` — an `N` (encoder)

Their composition is:

```
B encode_english decode_latin : Refined("latin/body", e) -> t
```

This says: "decode Latin prose to bits, then encode as English prose." It's the same `B N Adj` composition used at the object level for "the *big* (Adj) *house* (N)" — compose a modifier with a predicate.

Type checking confirms it:

```
B : (b -> c) -> (a -> b) -> a -> c
  encode_english : Bits -> t                  (b = Bits, c = t)
  decode_latin   : LatinText -> Bits          (a = LatinText, b = Bits)
  ─────────────────────────────────────────
  B encode_english decode_latin : LatinText -> t    ✓
```

### C — Flip (reverse direction)

```
C f x y = f(y)(x)
```

If `transcode` has type `Source -> Target -> t`, then `C transcode` flips the arguments:

```
transcode        : Source -> (Target -> t)
C transcode      : Target -> (Source -> t)
```

This converts "transcode *from* Latin *into* English" to "transcode *into* English *from* Latin" — the same pipeline, different argument order. The C combinator handles word-order variation at the object level (SOV vs. SVO); at the dialect level, it handles directionality.

### T — Type raising (lift data to pipeline)

```
T x f = f(x)
```

Given concrete data `nostr_sig : e` (a specific Nostr signature), type raising produces:

```
T nostr_sig : (e -> t) -> t
```

This lifts a specific piece of data into a function that accepts *any* encoder and applies it. "Here is a Nostr signature — give me any encoder and I'll feed the data through it."

At the object level, T raises a noun phrase to accept any verb phrase. At the dialect level, it raises data to accept any encoding pipeline.

### S — Distribution (fork)

```
S f g x = f(x)(g(x))
```

The S combinator applies two functions to the same input, then combines:

```
S transcode_english transcode_latin data = transcode_english(data)(transcode_latin(data))
```

This encodes the same data in two formats simultaneously — useful for generating both a Latin and English version from the same bitstream.

### I — Identity (passthrough)

```
I x = x
```

The identity encoder: data passes through unchanged. Useful as a default when no transformation is needed.

## Worked example: Nostr signature in Latin

"Encode a hex Nostr signature as Latin prose."

### Step 1: Assign dialect types

```
nostr_sig    : Refined("nostr/sig", e)                    -- the data (a Pron)
hex_decode   : Refined("hex", e) -> Refined("bits", e)    -- hex → bytes (an Adj)
encode_latin : Refined("bits", e) -> t                    -- bytes → Latin (an N)
hex_mode     : (e -> t) -> (e -> t)                       -- DataMode::Hex (a Det)
```

### Step 2: Build the pipeline

```
hex_mode(B encode_latin hex_decode)(nostr_sig) : t
```

Reading inside-out:

1. `hex_decode` transforms the hex representation to bits (`Adj : e -> e`)
2. `B encode_latin hex_decode` composes the Latin encoder after the decoder (`B N Adj : e -> t`)
3. `hex_mode(...)` wraps the composed encoder with the hex DataMode (`Det(N) : e -> t`)
4. Apply to `nostr_sig` to get the final output (`(e -> t)(e) : t`)

### Step 3: Type checking

```
hex_mode : (e -> t) -> (e -> t)         -- Det
B encode_latin hex_decode : e -> t       -- composed N
hex_mode(B encode_latin hex_decode) : e -> t
nostr_sig : e
─────────────────────────────────
hex_mode(B encode_latin hex_decode)(nostr_sig) : t   ✓
```

The type checker proves this pipeline is well-formed: the output is a valid encoding (`t`).

## Worked example: Latin to English transcoding

"Read Latin prose and output English prose."

```
decode_latin   : Refined("latin/body", e) -> Refined("bits", e)     -- Adj
encode_english : Refined("bits", e) -> t                             -- N

transcode = B encode_english decode_latin
          : Refined("latin/body", e) -> t
```

This is a single `B N Adj` application — the same combinator that composes adjectives with nouns in ordinary Montague Grammar.

## Worked example: PGP message in English

"Convert a PGP base64 message into English body text."

```
pgp_msg        : Refined("pgp/msg", e)                              -- Pron
base64_decode  : Refined("base64", e) -> Refined("bits", e)         -- Adj
encode_english : Refined("bits", e) -> t                             -- N
base64_mode    : (e -> t) -> (e -> t)                                -- Det

base64_mode(B encode_english base64_decode)(pgp_msg) : t
```

## The bitstream as zero object

Every dialect `D` that carries payload is isomorphic to the bitstream:

```
encode_D : Bits -> D
decode_D : D -> Bits
decode_D . encode_D = id_Bits
encode_D . decode_D = id_D
```

This means every morphism between dialects factors through bits:

```
hom(D1, D2) = {encode_D2 . decode_D1}
```

In category-theoretic terms, `Bits` is a **zero object** in the category of dialects — every dialect has exactly one morphism to and from it. The B combinator computes these factored morphisms.

This is why Glossia translation is always lossless and commutative (see [Bijectivity](./how-it-works.md#bijectivity)):

```
A -> B -> C  =  A -> C
```

The dialect calculus makes this algebraic structure explicit.

## DataMode as a natural transformation

The four DataModes (`Bytes8`, `Ascii7`, `Base64`, `Hex`) are not encoders themselves — they are **natural transformations** between encoding functors.

A DataMode `m` transforms any encoder `enc : e -> t` into a modified encoder `m(enc) : e -> t` that pre-processes the input. In the dialect calculus, this is exactly the `Det` type:

```
Det : (e -> t) -> (e -> t)
```

The naturality condition says: applying a DataMode and then transcoding gives the same result as transcoding and then applying the DataMode. This holds because DataModes operate on the bits, which are invariant under transcoding:

```
B (m(encode_B)) decode_A  =  m(B encode_B decode_A)
```

## Connection to Curry-Howard

The [Curry-Howard correspondence](https://en.wikipedia.org/wiki/Curry%E2%80%93Howard_correspondence) is the observation that **types are propositions** and **programs are proofs**. A function of type `A -> B` doesn't just transform data — it *proves* that `A` implies `B`. Type-checking a program is the same as verifying a proof. If the types don't line up, the "proof" is invalid — you've asserted something that doesn't follow.

This isn't a metaphor. Curry noticed in 1934 that combinator types match axiom schemes of intuitionistic logic. Howard made it precise in 1969: natural deduction proofs correspond exactly to typed lambda terms. Every logical connective has a computational counterpart — conjunction is pairing, disjunction is tagged union, implication is function abstraction. See Philip Wadler's [Propositions as Types](https://homepages.inf.ed.ac.uk/wadler/papers/propositions-as-types/propositions-as-types.pdf) for an accessible treatment of the full story.

The dialect calculus inherits this correspondence directly. Each column below is the *same* formal object seen from three angles:

| Lambda calculus | Logic | Dialect calculus |
|----------------|-------|-----------------|
| Type `A -> B` | Proposition "A implies B" | "Given data in format A, I can produce format B" |
| Term `f : A -> B` | Proof of "A implies B" | A concrete encoder/decoder |
| Application `f(x)` | Modus ponens | Running the pipeline on data |
| Composition `B f g` | Transitivity of implication | Transcoding via intermediate representation |
| `t` (truth) | Theorem | Valid encoded output exists |
| Type error | Contradiction | Incompatible pipeline (wrong wordlist, wrong DataMode) |

### What this buys us

**1. Type-checking is proof verification.** When the dialect calculus type-checks a pipeline like

```
hex_mode(B encode_latin hex_decode)(nostr_sig) : t
```

it is simultaneously verifying a *proof* that "given a Nostr hex signature and a hex decoder and a Latin encoder, a valid Latin encoding exists." If any component has the wrong type — say you accidentally pass a Latin decoder where a hex decoder is expected — the proof fails. The type system catches the error before any data flows.

**2. Composition is transitivity.** The `B` combinator computes `B f g x = f(g(x))`. Logically, if `g` proves `A => B` and `f` proves `B => C`, then `B f g` proves `A => C`. This is why transcoding works: `decode_latin` proves "Latin implies Bits" and `encode_english` proves "Bits implies English", so their composition proves "Latin implies English." The proof factors through the bitstream, just as the data does.

**3. Uninhabited types mean impossible pipelines.** If no term of type `Refined("hex", e) -> Refined("latin/body", e)` exists (because there's no direct hex-to-Latin transform), then the corresponding proposition "hex implies Latin" has no proof. You *must* factor through `Bits` — and the type system forces you to make this explicit by composing `B encode_latin hex_decode`.

**4. The zero object is the tautology.** `Bits` being a zero object (every dialect has exactly one morphism to and from it) means that `Bits => D` and `D => Bits` are both provable for every dialect `D`. In logical terms, `Bits` is equivalent to every well-formed dialect — it's the canonical witness that all lossless encodings preserve the same information.

## Meta-grammar

The meta-language for dialect pipelines uses the **same CFG** as the object language. A pipeline specification is a sentence in the meta-grammar:

```
S  -> NP VP Dot
NP -> Det N              "the hex encoder"
NP -> Adj N              "the Latin decoder"
VP -> V NP               "transcodes into English"
VP -> V PP               "encoded from Latin"
PP -> Prep NP            "from Latin" / "into English"
```

The semantic derivation of this meta-sentence IS the pipeline specification. The same type-driven grammar that validates "the cat sees the dog" also validates "the hex decoder transcodes into Latin prose."

This self-similarity — the meta-language having the same grammar as the object language — is not a coincidence. It follows from the fact that both levels use the same type algebra (`e`, `t`, `->`, `Refined`). The grammar is a free algebra over these types; lifting the interpretation of `e` and `t` lifts the entire grammar.

## Summary

| Concept | Object level (words → sentences) | Dialect level (formats → pipelines) |
|---------|--------------------------------|-------------------------------------|
| `e` | Individual entity | Data in some representation |
| `t` | Valid sentence | Valid encoded output |
| `N : e -> t` | Noun (predicate) | Encoder |
| `V : e -> (e -> t)` | Verb (relation) | Transcoder |
| `Adj : e -> e` | Adjective (modifier) | Format transform (decode) |
| `Det : (e -> t) -> (e -> t)` | Determiner (quantifier) | DataMode (codec modifier) |
| `Conj : t -> (t -> t)` | Conjunction | Output combiner |
| `B f g` | Compose modifier + predicate | Transcode (decode then encode) |
| `C f` | Flip argument order | Reverse pipeline direction |
| `T x` | Type-raise entity | Lift data to accept any encoder |
| `S f g` | Distribute over argument | Fork: encode same data two ways |
| `Refined(r, A)` | Language-specific feature | Dialect identity |
