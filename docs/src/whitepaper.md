# Glossia: A Type-Theoretic Framework for Encoding Binary Data in Human-Friendly Media

**Abstract.** We present Glossia, a framework that encodes arbitrary binary data into grammatically correct natural language, aesthetically rendered images, structured musical scores, and prime-factorization arithmetic. At its core, Glossia uses Montague Grammar --- a typed lambda calculus over part-of-speech categories --- to guarantee that every encoding is well-formed according to the rules of its target medium. A context-free grammar (CFG) generates structural frames; payload words carrying data are embedded into type-compatible slots; cover words fill the remainder. Decoding is trivial: filter the output against the payload wordlist. We show that this architecture generalizes beyond prose to any medium where a compositional grammar can be defined over a finite vocabulary, and that a meta-language built from the same type algebra provides type-safe pipelining between all supported media.

---

## 1. Introduction

The problem of representing binary data in human-readable form is as old as computing itself. Base64 encodes bytes as ASCII characters. BIP39 maps 11-bit chunks to English words for cryptocurrency seed phrases. QR codes render bits as black-and-white modules. Each solution is domain-specific: the encoding format, the vocabulary, and the output structure are fused into a single design.

Glossia takes a different approach. Rather than designing a new encoding for each medium, it provides a *universal framework* in which the encoding vocabulary (payload words), the structural grammar (CFG productions), and the output aesthetics (rendering) are independently composable parameters. The key insight is that **Montague Grammar** --- originally developed to give natural language the same formal rigor as mathematical logic --- provides exactly the type system needed to ensure that data-carrying words are placed only in grammatically valid positions.

The result is a system where:

- A 12-word BIP39 seed phrase becomes a paragraph of grammatically correct English prose.
- The same seed phrase becomes a passage of Latin, a Voronoi diagram of perceptually distinct colors, a sequence of MIDI notes in a pentatonic scale, or a Merkle tree of prime numbers.
- Translation between any two representations is lossless, because all representations factor through a common binary intermediate.
- A meta-language, itself governed by the same type algebra, specifies pipelines like "translate from English into Latin" as grammatically valid sentences whose semantic derivation *is* the pipeline specification.

This paper develops the theory, describes the architecture, and presents applications across four media: human language, images, music, and mathematics.

## 2. Theoretical Foundations

### 2.1 Montague Grammar and Typed Lambda Calculus

Richard Montague's central thesis (1970) was that natural language can be described by the same mathematical tools as formal logic. His grammar assigns every word a *semantic type* drawn from a simple type theory:

- **e** --- the type of entities (individuals, things)
- **t** --- the type of truth values (propositions, sentences)
- **A -> B** --- the type of functions from A to B

A noun like "cat" has type `e -> t` (a predicate: given an entity, it returns a truth value --- "is this entity a cat?"). A transitive verb like "sees" has type `e -> (e -> t)` (a relation: given an object entity and a subject entity, it produces a proposition). A determiner like "the" has type `(e -> t) -> (e -> t)` (a quantifier: it takes a predicate and returns a modified predicate).

Sentence construction is *function application*. "The cat sees the dog" is built by:

1. Applying "the" `((e -> t) -> (e -> t))` to "cat" `(e -> t)` to get "the cat" `(e -> t)`.
2. Applying "sees" `(e -> (e -> t))` to "the dog" `(e -> t)` to get "sees the dog" `(e -> t)`.
3. Applying "sees the dog" to "the cat" to get a truth value `t`.

A sequence of words that reduces to type `t` is a well-formed sentence. A sequence that does not reduce to `t` is ungrammatical. **Type-checking is grammaticality checking.**

### 2.2 Parts of Speech as Semantic Types

Glossia maps each part-of-speech (POS) tag to a Montague type:

| POS | Type | Linguistic Interpretation |
|-----|------|--------------------------|
| N (Noun) | `e -> t` | Predicate over entities |
| V (Verb) | `e -> (e -> t)` | Relation between entities |
| Adj (Adjective) | `e -> e` | Entity modifier |
| Adv (Adverb) | `(e -> t) -> (e -> t)` | Predicate modifier |
| Det (Determiner) | `(e -> t) -> (e -> t)` | Quantifier |
| Prep (Preposition) | `e -> (e -> t)` | Prepositional relation |
| Conj (Conjunction) | `t -> (t -> t)` | Proposition connective |
| Cop (Copula) | `(e -> t) -> (e -> t)` | Subject-predicate linker |
| Modal/Aux/To | `(e -> t) -> (e -> t)` | Predicate modifier |
| Pron (Pronoun) | `e` | Entity reference |
| Dot (Period) | `t` | Sentence terminator |

This type assignment is language-independent. English, Latin, French, and German all use the same type algebra; they differ only in their CFG productions (word order), their vocabularies, and their morphological features (e.g., Latin has no determiners).

### 2.3 CCG Combinators

Glossia augments Montague Grammar with five combinators from Combinatory Categorial Grammar (CCG):

| Combinator | Definition | Type | Role |
|-----------|-----------|------|------|
| **B** (Composition) | `B f g x = f(g(x))` | `(b->c) -> (a->b) -> a -> c` | Compose modifier with predicate |
| **C** (Permutation) | `C f x y = f(y)(x)` | `(a->b->c) -> b -> a -> c` | Handle word-order variation |
| **T** (Type raising) | `T x f = f(x)` | `a -> (a->b) -> b` | Lift entity to accept any predicate |
| **S** (Distribution) | `S f g x = f(x)(g(x))` | `(a->b->c) -> (a->b) -> a -> c` | Share argument between functions |
| **I** (Identity) | `I x = x` | `a -> a` | Passthrough |

These combinators enable flexible word order (the C combinator handles SOV vs. SVO), composition of modifiers (the B combinator composes adjectives with nouns), and argument sharing (the S combinator applies two functions to the same input).

### 2.4 Refinement Types

To distinguish between dialects and language-specific features, Glossia extends the base type system with *refinement types*:

```
Refined("latin/body", e)    -- Latin prose as a data representation
Refined("hex", e)           -- Hexadecimal string
Refined("bits", e)          -- Raw bitstream
Refined("pentatonic/C", e)  -- Pentatonic scale rooted at C
```

Refinement subsumption follows a slash-hierarchical convention: `e` accepts `e[latin]`, and `e[latin]` accepts `e[latin/body]`, but `e[latin]` does not accept `e[english]`. This gives the type system the precision to distinguish between dialects while allowing generic operations to work across all dialects of a family.

## 3. Architecture

### 3.1 Two-Layer Grammar

Glossia uses a two-layer grammar system:

**Layer 1: Context-Free Grammar (CFG).** Weighted production rules generate sequences of POS tags. For example:

```
S   -> NP VP Dot          [weight 0.85]
NP  -> Det N              [weight 0.50]
NP  -> Det Adj N          [weight 0.10]
VP  -> Modal V NP         [weight 0.30]
VP  -> Cop Adj            [weight 0.15]
```

Weights determine probabilistic selection between alternative productions. A *compact* mode selects the shortest sentence that fits the payload; a *natural* mode samples from the grammar's length distribution for more varied output.

**Layer 2: Type-Driven Constraints.** Each CFG production has an associated lambda term that specifies how its constituents compose semantically. The type checker verifies that the lambda term is well-typed --- that the POS sequence can reduce to type `t`. This prevents the CFG from generating nonsensical combinations like `Det Det V` that would be syntactically derivable but semantically ill-formed.

Grammar rules are defined in YAML:

```yaml
noun_phrase:
  lambda: "λn:(e->t). n"
  cfg_productions:
    - production: "Det[def] Adj N"
      weight: 0.10
      lambda: "λn:(e->t). λadj:(e->e). λdet:((e->t)->(e->t)). det(n)"
```

### 3.2 Payload and Cover Wordlists

Every Glossia language maintains two disjoint wordlists:

- **Payload wordlist**: Words that carry data. Each word maps to a unique index; the index represents a binary value. For a wordlist of size $2^b$, each word encodes $b$ bits. BIP39's 2048-word list gives 11 bits per word. Glossia's Latin vocabulary of $2^{16}$ words gives 16 bits per word.

- **Cover wordlist**: Function words that provide grammatical scaffolding but carry no data. Determiners ("the", "a"), copulas ("is", "are"), modals ("may", "can"), and sentence terminators (".") are cover words.

POS slots are partitioned into *payload-eligible* slots (N, V, Adj, Adv, Prep, Det, Conj) and *cover-only* slots (Aux, Cop, To, Prefix, Dot, Modal). Payload words are POS-tagged with probability distributions across eligible categories, enabling flexible placement.

**Both wordlists are strictly append-only.** Since decoding maps words to their positional indices, reordering or removing words would corrupt previously encoded messages. Appending new words increases future encoding density without breaking backward compatibility.

### 3.3 Encoding Process

Given a payload (a sequence of BIP39 words, an ASCII string, a hex key, or raw bytes):

1. **Bit-pack the input.** Convert the input to a bitstream. Map consecutive $b$-bit chunks to payload word indices. The payload is now a sequence of words.

2. **POS-tag the payload.** Each payload word has a probability distribution over POS categories. The generator knows which slots each word can fill.

3. **Generate a POS frame.** The CFG produces a sequence of POS slots (e.g., `Det N V Det Adj N Dot`). The type checker verifies the frame is well-formed.

4. **Embed payload words.** A maximum subsequence matching algorithm places payload words into compatible POS slots, maintaining their original order. Cover words fill the remaining slots.

5. **Repeat.** If payload words remain after one sentence, generate another sentence and continue embedding.

### 3.4 Decoding

Decoding is deliberately trivial: **filter the output text against the payload wordlist.** Every word that appears in the payload wordlist is a payload word; everything else is cover. The extracted payload words, in order, are the encoded message.

This asymmetry --- complex encoding, trivial decoding --- is a design goal. The grammar, the cover words, and the sentence structure are an "aesthetic wrapper" that is discarded entirely during decoding. The wrapper serves human readability; the payload serves machine fidelity.

### 3.5 Bijectivity and Translation

Because all information resides in the payload word sequence, and payload words biject with bit indices, **every Glossia encoding is a bijection between bitstreams and word sequences.** This yields several properties:

- **Lossless round-trip**: Encode then decode recovers the original bitstream exactly.
- **Lossless translation**: Decode from language A to bits, encode from bits to language B. No information is lost.
- **Commutativity**: Translating A -> B -> C gives the same result as A -> C, because both factor through the same bitstream.

The bitstream is the **zero object** in the category of Glossia dialects: every dialect has exactly one encoding morphism to and from the bitstream, and every translation factors through it.

### 3.6 Merkle Mode

Glossia supports a Merkle tree mode that wraps $N$ payload words in a cryptographic proof structure. The $N$ payload words become leaves of a binary Merkle tree; $N-1$ cover words are assigned as internal nodes (most frequent cover word becomes the root). A pre-order traversal produces the output sequence of $2N-1$ words.

This provides:
- **Tamper detection**: The Merkle structure proves the payload words are authentic and in the correct order.
- **Natural appearance**: Common words like "the" appear as the root and internal nodes, maintaining the illusion of natural prose.
- **Steganographic Merkle proofs**: The proof structure is hidden within the grammatical scaffolding.

## 4. Application: Human Language Encoding

### 4.1 English

English is Glossia's default and most developed language. The grammar supports multiple dialects:

- **Body**: Full sentences with natural variation (`NP VP Dot`, `NP Adv V NP Dot`). Optimized for email body text and prose.
- **Subject**: Shorter phrases (`NP VP`, `Adj N VP PP`). Optimized for email subject lines.
- **Prose**: Flowing sentences with PP chains, conjunctions, and complex NPs for paragraph-length output.

Example encoding of a 12-word BIP39 seed phrase:

```
Input:  snake robot mixed ship program night ten shield bamboo own way yellow
Output: Snake set robot to the mixed ship. Each program is night to each
        ten shield. Bamboo own way to the yellow.
```

The embedded payload words appear in grammatically natural positions. The cover words ("set", "to", "the", "each", "is") provide syntactic glue. Decoding extracts exactly the 12 original words.

### 4.2 Latin

Latin demonstrates Glossia's cross-linguistic capability. Key differences from English:

- **No determiners**: Latin has no articles. The grammar omits Det entirely, yielding shorter, denser encodings.
- **Flexible word order**: Case endings handle grammatical relations, so the CFG can produce SOV, SVO, and other orders.
- **Larger vocabulary**: With $2^{16} = 65{,}536$ payload words, each Latin word encodes 16 bits --- compared to BIP39's 11 bits. A 132-bit seed phrase requires only 9 Latin words instead of 12 English words.

The same Montague types apply: Latin nouns are still `e -> t`, verbs are still `e -> (e -> t)`. Only the CFG productions change to reflect Latin syntax.

### 4.3 Information Density

| Language | Payload Size | Bits/Word | Words for 128-bit Key |
|----------|-------------|-----------|----------------------|
| English (BIP39) | 2,048 | 11 | 12 |
| Latin | 65,536 | 16 | 8 |
| English (expanded) | 4,096 | 12 | 11 |

Larger vocabularies increase bits per word, yielding more compact encodings. Because wordlists are append-only, expanding a vocabulary increases density for future encodings without breaking backward compatibility.

## 5. Application: Image Encoding

### 5.1 Color-Space Steganography

The Glossia image codec encodes binary data into images using a geometric construction in CIELAB perceptual color space. The core idea:

**A color palette defines a 1D curve $\gamma$ through CIELAB.** At each palette point, a 2D constellation grid in the normal plane encodes additional bits. Every pixel color decomposes into:

- **Tangential position** on $\gamma$ --- identifies the payload word (which palette color).
- **Normal-plane displacement** --- encodes the sequence position (which occurrence of that word).

The rendering (Voronoi diagram, pixel grid, mosaic, brush strokes) is purely aesthetic. **All information lives in the multiset of colors**, not in spatial arrangement.

### 5.2 Geometric Construction

**Palette Curve.** A cubic spline through $K$ control points in CIELAB, reparameterized by arc length. $N$ palette colors are equally spaced along $\gamma$:

$$c_i = \gamma\!\left(\frac{i \cdot L}{N-1}\right), \quad i = 0, \ldots, N-1$$

**Bishop Frame.** An orthonormal frame $\{T(s), U_1(s), U_2(s)\}$ propagated along $\gamma$ via double-reflection (Wang et al., 2008). The Bishop frame is smooth through inflection points, unlike the Frenet frame.

**Constellation Grid.** At each palette point $c_i$, an $M_i \times M_i$ grid in the normal plane $\text{span}\{U_1, U_2\}$:

$$\text{point}(a, b) = c_i + \alpha_a \cdot U_1(s_i) + \alpha_b \cdot U_2(s_i)$$

The grid size $M_i$ is determined by the local tube radius --- the largest inscribed disk in the normal plane that stays within the sRGB gamut.

**Bits per cell:**

$$\text{bits/cell} = \log_2 N + 2 \log_2 M_{\min}$$

With $N = 16$ palette colors and grid spacing $\varepsilon = 2.3$ CIELAB (the just-noticeable difference), each visual element carries 12 bits.

### 5.3 Decoding

Three decoders are implemented, forming a Pareto frontier:

| Decoder | Complexity | Clean Accuracy | Needs Layout? |
|---------|-----------|----------------|---------------|
| Geometric | $O(nS)$ | 100% | Yes |
| Rips filtration | $O(n^2 \log n)$ | 100% | No |
| Spectral | $O(n^3)$ | 92.5% | No |

The **geometric decoder** projects each pixel onto $\gamma$, snaps to the nearest palette color, decomposes the residual via the Bishop frame, and snaps to the constellation grid.

The **Rips filtration decoder** uses persistent homology: project all colors onto $\gamma$, build a Vietoris-Rips filtration, and detect the persistence gap that separates within-word merges from between-word merges. The persistence gap equals the palette spacing --- a geometric invariant.

The **spectral decoder** constructs a Gaussian similarity graph on 1D projections, computes the normalized graph Laplacian, and uses the eigengap to determine the number of distinct words.

### 5.4 Noise Tolerance and Comparison with QR Codes

Spatial averaging over $P$ pixels per visual element reduces effective noise by $\sqrt{P}$:

$$\sigma_{\text{eff}} = \frac{\sigma_{\text{raw}}}{\sqrt{P}}$$

For a 32-byte Nostr public key at 400x400 resolution:

| Configuration | Elements | Bits/Cell | $\sigma_{95,\text{eff}}$ |
|--------------|----------|-----------|------------------------|
| QR-L V2 (25x25) | 625 modules | 1 | 18 |
| Voronoi, $\varepsilon$=2.3, 50% ECC | 32 cells | 12 | 110 |

Glossia's image codec achieves **6x greater noise tolerance** than QR codes while using **20x fewer visual elements**, yielding aesthetically rich images rather than black-and-white grids.

### 5.5 Screen-to-Camera Channel

For the real-world scenario of photographing an encoded image displayed on a screen, the codec includes calibration cells (one per palette color) that enable color-space-only calibration. A 4-parameter affine camera model captures auto white balance, exposure, and saturation:

$$\begin{pmatrix} L' \\ a' \\ b' \end{pmatrix} = \begin{pmatrix} 1 & 0 & 0 \\ 0 & s & 0 \\ 0 & 0 & s \end{pmatrix} \begin{pmatrix} L \\ a \\ b \end{pmatrix} + \begin{pmatrix} \Delta L \\ \Delta a \\ \Delta b \end{pmatrix}$$

Grid search over the 4-parameter space ($\sim$53,000 evaluations) identifies both the palette and the camera correction simultaneously. No fiducials, markers, or known pixel positions are required.

## 6. Application: Music Encoding

### 6.1 MIDI Notes as Payload Words

Glossia's music language encodes binary data as sequences of MIDI note names. The full chromatic set of 128 notes (C-1 through G9) provides 7 bits per note. The type algebra reinterprets POS categories for music:

| POS | Musical Interpretation |
|-----|-----------------------|
| N | Payload note (carries data) |
| Adj | Duration (whole, half, quarter, eighth, sixteenth) |
| Adv | Dynamics (pp, p, mp, mf, f, ff) |
| Dot | Barline or double barline |
| Cop | Rest |
| Modal | Articulation (legato, staccato) |
| Aux | Header tokens (tempo, time signature) |

The CFG generates musical structure:

```
sentence -> BARS Dot[end]
BAR      -> BEAT BEAT BEAT BEAT Dot      (4/4 time)
BEAT     -> N Adj                         (note + duration)
BEAT     -> Cop Adj                       (rest + duration)
BEAT     -> Adv N Adj                     (dynamic + note + duration)
```

### 6.2 Scale-Derived Dialects

Musical scales are defined by interval patterns. A pentatonic scale `[2, 2, 3, 2, 3]` rooted at C produces pitch classes {C, D, E, G, A}. Applied across all octaves, this yields 45 notes from the 128-note chromatic set.

Scale dialects are specified declaratively in grammar YAML:

```yaml
pentatonic:
  parent: raw
  scale:
    intervals: [2, 2, 3, 2, 3]
    root: C
```

The system derives the filtered payload wordlist at build time --- no static file needed. Available scales include major pentatonic, minor pentatonic, and blues. Adding a new scale requires only a YAML stanza.

Refinement types ensure type safety: a pentatonic dialect's N slots carry refinement `pentatonic/C`, preventing accidental mixing with chromatic or blues notes.

### 6.3 Dialects

- **Raw**: Notes only, no barlines or headers. Maximum payload density.
- **Scored**: Header with tempo and time signature, structured bars with barlines, variable bar lengths (4-beat, 3-beat, 2-beat) for musical expressiveness.

The scored dialect's variable bar lengths represent expressive timing, not literal time-signature changes --- a deliberate aesthetic choice that makes the output sound more musical while maintaining the same payload capacity.

## 7. Application: Mathematics (Prime Encoding)

### 7.1 Primes as Payload Words

The `math/primes` language uses prime numbers as payload words. The Merkle tree construction maps a list of leaf primes into a cryptographically structured sequence:

1. **Input**: A list of $n$ leaf primes $[p_1, p_2, \ldots, p_n]$.
2. **Generate Merkle primes**: $n-1$ primes strictly greater than $\max(p_i)$.
3. **Build tree bottom-up**: Pair leaves into internal nodes.
4. **Assign primes top-down**: Root gets the largest Merkle prime, descending by BFS level.
5. **Pre-order traversal**: Output $[root, left\_subtree, right\_subtree]$.

The grammar maps Merkle tree structure to POS sequences:

| Merkle Concept | POS | Type |
|---------------|-----|------|
| Root | N | `e -> t` |
| Internal node | V | `e -> (e -> t)` |
| Leaf | Det | `e -> t` |

Example productions:

```
N Det Det           -- 2-leaf tree
N V Det Det Det     -- 3-leaf tree
N V Det Det V Det Det  -- 4-leaf balanced tree
```

### 7.2 Prime Ordering Constraint

Cover words in the prime language are composite (non-prime) integers, constrained by:

$$p_{\text{left}} < w_{\text{cover}} < p_{\text{right}}$$

where $p_{\text{left}}$ and $p_{\text{right}}$ are adjacent payload primes. This preserves the ordering invariant needed for deterministic Merkle tree reconstruction. For primes $[3, 5]$, the only valid cover word is 4.

### 7.3 Properties

- **Disjoint sets**: All Merkle primes are greater than all leaf primes, enabling deterministic identification.
- **Order preservation**: Input order is maintained at the leaf level.
- **Self-verifying**: The Merkle structure provides a built-in integrity proof.

The lambda expression for the complete merkleization process:

```
merkleize = λleaves: List[Prime].
  let max_leaf = fold(max, leaves) in
  let n = length(leaves) in
  let merkle_primes = generate_primes_after(max_leaf, n-1) in
  let tree = build_tree(map(leaf, leaves), merkle_primes) in
  let tree_assigned = assign_bfs(tree, reverse(merkle_primes)) in
  preorder(tree_assigned)
```

This maps cleanly to the type system: `merkleize : List[Prime] -> List[Prime]`, transforming $n$ leaf primes into $2n-1$ pre-order traversal primes.

## 8. The Meta-Language and Pipeline Algebra

### 8.1 Dialects as Types

Glossia's most powerful abstraction is its meta-language: a grammar built from the *same* type algebra as the object-level grammars, but with types reinterpreted at the dialect level:

| Object Level | Dialect Level |
|-------------|---------------|
| `e` = an entity | `e` = a data representation (hex, Latin text, bits) |
| `t` = a valid sentence | `t` = a valid encoded output |
| `N : e -> t` = noun (predicate) | `N` = encoder (data -> output) |
| `V : e -> (e -> t)` = verb (relation) | `V` = transcoder (source -> encoder) |
| `Adj : e -> e` = adjective (modifier) | `Adj` = format transform (decode) |
| `Det : (e -> t) -> (e -> t)` = determiner | `Det` = DataMode (hex, base64, ascii7) |
| `Prep : e -> (e -> t)` = preposition | `Prep` = direction ("from", "into") |

### 8.2 Pipeline Specifications as Sentences

A pipeline specification is a sentence in the meta-grammar:

```
"translate from english into latin"
          ^^^^           ^^^^
          source         target
```

The meta-grammar uses the same CFG format as object grammars:

```yaml
sentence:
  cfg_productions:
    - production: "NP VP Dot"       # "encode <data> as <target>."
    - production: "VP PP PP Dot"    # "transcode from <source> into <target>."
```

The semantic derivation of this meta-sentence *is* the pipeline specification. The same type-driven grammar that validates "the cat sees the dog" also validates "the hex decoder transcodes into Latin prose."

### 8.3 CCG Combinators as Pipeline Algebra

The five CCG combinators have direct pipeline interpretations:

**B (Composition) = Transcode.** Given `decode_latin : Latin -> Bits` and `encode_english : Bits -> t`, their B-composition is:

```
B encode_english decode_latin : Latin -> t
```

"Decode Latin to bits, then encode as English." This is the same `B N Adj` composition used for "the big house" --- compose a modifier with a predicate.

**C (Permutation) = Reverse direction.** `C transcode` flips "from Latin into English" to "into English from Latin."

**T (Type raising) = Lift data to pipeline.** Given data `nostr_sig : e`, type raising produces `T nostr_sig : (e -> t) -> t` --- "here is a Nostr signature; give me any encoder."

**S (Distribution) = Fork.** `S transcode_english transcode_latin data` encodes the same data in two formats simultaneously.

### 8.4 The Bitstream as Zero Object

Every dialect $D$ that carries payload is isomorphic to the bitstream:

$$\text{encode}_D : \text{Bits} \to D, \quad \text{decode}_D : D \to \text{Bits}$$

$$\text{decode}_D \circ \text{encode}_D = \text{id}_{\text{Bits}}, \quad \text{encode}_D \circ \text{decode}_D = \text{id}_D$$

Every morphism between dialects factors through bits:

$$\text{hom}(D_1, D_2) = \{\text{encode}_{D_2} \circ \text{decode}_{D_1}\}$$

In category-theoretic terms, `Bits` is a **zero object** in the category of dialects. This is why translation is always lossless and commutative: $A \to B \to C = A \to C$.

### 8.5 DataMode as Natural Transformation

The four DataModes (Bytes8, Ascii7, Base64, Hex) are natural transformations between encoding functors. A DataMode $m$ transforms any encoder `enc : e -> t` into `m(enc) : e -> t`. In the type system, this is exactly the `Det` type: `(e -> t) -> (e -> t)`.

The naturality condition states that applying a DataMode commutes with transcoding:

$$B \; (m(\text{encode}_B)) \; \text{decode}_A = m(B \; \text{encode}_B \; \text{decode}_A)$$

This holds because DataModes operate on bits, which are invariant under transcoding.

### 8.6 Curry-Howard Correspondence

The dialect calculus inherits the Curry-Howard correspondence directly:

| Lambda Calculus | Logic | Dialect Calculus |
|----------------|-------|-----------------|
| Type `A -> B` | "A implies B" | "Given format A, I can produce format B" |
| Term `f : A -> B` | Proof of "A implies B" | A concrete encoder/decoder |
| Application `f(x)` | Modus ponens | Running the pipeline on data |
| Composition `B f g` | Transitivity | Transcoding via intermediate |
| `t` | Theorem | Valid encoded output exists |
| Type error | Contradiction | Incompatible pipeline |

**Type-checking is proof verification.** When the type checker validates:

```
hex_mode(B encode_latin hex_decode)(nostr_sig) : t
```

it simultaneously verifies that "given a Nostr hex signature, a hex decoder, and a Latin encoder, a valid Latin encoding exists." If any component has the wrong type, the proof fails before any data flows.

**Uninhabited types mean impossible pipelines.** If no term of type `Refined("hex", e) -> Refined("latin/body", e)` exists, then you *must* factor through `Bits`. The type system forces this to be explicit.

### 8.7 Worked Example: Nostr Signature in Latin

"Encode a hex Nostr signature as Latin prose."

**Step 1.** Assign dialect types:

```
nostr_sig    : Refined("nostr/sig", e)                -- the data
hex_decode   : Refined("hex", e) -> Refined("bits", e) -- hex -> bytes
encode_latin : Refined("bits", e) -> t                  -- bytes -> Latin
hex_mode     : (e -> t) -> (e -> t)                     -- DataMode::Hex
```

**Step 2.** Build the pipeline:

```
hex_mode(B encode_latin hex_decode)(nostr_sig) : t
```

**Step 3.** Type-check:

```
hex_mode : (e -> t) -> (e -> t)
B encode_latin hex_decode : e -> t
hex_mode(B encode_latin hex_decode) : e -> t
nostr_sig : e
────────────────────────────────────────────
hex_mode(B encode_latin hex_decode)(nostr_sig) : t   ✓
```

The pipeline is well-formed. The output is valid Latin prose encoding the Nostr signature.

## 9. Self-Similarity of Object and Meta Levels

A striking property of Glossia's architecture is that **the meta-language has the same grammar as the object language.** This is not a coincidence: both levels use the same type algebra (`e`, `t`, `->`, `Refined`). The grammar is a free algebra over these types. Lifting the interpretation of `e` from "entity" to "data representation" lifts the entire grammar.

This self-similarity means:

1. **One type checker serves both levels.** The same Rust code that validates English sentences validates pipeline specifications.
2. **One parser serves both levels.** The lambda parser, the CFG engine, and the POS tagger are reused without modification.
3. **The meta-language is itself a Glossia language.** It has its own payload wordlist (dialect names: "latin", "english", "hex", "bits"), its own cover wordlist (pipeline verbs: "encode", "decode", "translate", "from", "into"), and its own grammar productions.
4. **Meta-meta levels are possible.** A grammar that describes how to compose meta-language pipelines would use the same type system. The tower of grammars is limited only by practical utility, not by architectural constraints.

## 10. Error Correction and Transmission

Glossia is a pure encoding layer: it faithfully represents whatever bits it is given, without assumptions about what those bits mean. This design separates concerns cleanly:

- **Checksums, error correction codes, and metadata** are encoded as part of the payload bitstream itself.
- **Glossia handles encoding/decoding**: the language layer converts bits to readable form and back.
- **Your protocol handles integrity**: checksums and error correction verify the decoded bits.

For the image codec specifically, Reed-Solomon error correction wraps the parametric encoder for fair comparison against QR codes. The RS layer is optional and additive --- it increases the number of visual elements but enables correction of residual decoding errors.

## 11. Implementation

Glossia is implemented in Rust with the following key components:

| Module | Purpose |
|--------|---------|
| `semantic_types.rs` | `SemanticType` enum, refinement subsumption, type application |
| `lambda_terms.rs` | `LambdaTerm` enum, CCG combinators, beta reduction |
| `lambda_parser.rs` | Pest-based parser for lambda expressions |
| `type_driven_grammar.rs` | `LanguageConfig`, `TypeRule`, generation from types |
| `grammar.rs` | CFG parser, weighted production selection |
| `codec.rs` | Bit-packing, DataMode encoding/decoding |
| `pipeline.rs` | Meta-language-driven translation between languages |
| `merkle.rs` | Append-only wordlist trees, Merkle proof construction |
| `scale.rs` | Musical scale derivation from interval patterns |

Language definitions reside in `languages/{language}/grammar.yaml` with associated wordlists. All language files are embedded at compile time via `build.rs`, with debug builds embedding only English for fast iteration.

The system compiles to a single binary with no runtime file dependencies. A WebAssembly target enables browser-based encoding and decoding.

## 12. Related Work

**BIP39** (Bitcoin Improvement Proposal 39) established the practice of encoding cryptographic entropy as natural-language word sequences. Glossia generalizes BIP39 by adding grammatical structure and cross-language translation.

**Montague Grammar** (Montague, 1970) provided the theoretical foundation for treating natural language with the formal rigor of mathematical logic. Glossia applies Montague's type system operationally, as a constraint on data encoding rather than a tool for semantic analysis.

**Combinatory Categorial Grammar** (Steedman, 2000) extended Montague's approach with combinators for flexible word order and composition. Glossia uses the B, C, S, T, I combinators for both sentence construction and pipeline specification.

**Steganography** research has explored hiding data in text (linguistic steganography) and images. Glossia's image codec differs from traditional steganography in that it is not adversarial --- the goal is aesthetic human-readable encoding, not hidden communication.

**QR codes** (ISO/IEC 18004) encode binary data as 2D black-and-white matrices. Glossia's image codec achieves higher information density per visual element and greater noise tolerance, at the cost of requiring a color-capable display and camera.

## 13. Conclusion

Glossia demonstrates that a single type-theoretic framework --- Montague Grammar extended with CCG combinators and refinement types --- can govern the encoding of binary data into arbitrarily diverse media. The key contributions are:

1. **A universal encoding architecture** where vocabulary, grammar, and rendering are independently composable parameters, connected by a shared type system.

2. **A proof that grammaticality checking and type-checking are the same operation**, enabling the same infrastructure to validate English sentences, Latin prose, musical scores, Merkle trees, and pipeline specifications.

3. **A meta-language** that uses the identical type algebra to compose encoding pipelines, making translation between media type-safe and algebraically composable.

4. **Concrete applications** across four media --- human language, images, music, and mathematics --- demonstrating that the framework is not merely theoretical but practically effective.

The bitstream as zero object, the Curry-Howard correspondence between pipeline type-checking and proof verification, and the self-similarity between object and meta levels suggest that Glossia is not just a tool but a *calculus of representation* --- a formal language for talking about how information can be presented to humans.

---

## References

- Montague, R. (1970). "Universal Grammar." *Theoria*, 36(3), 373--398.
- Steedman, M. (2000). *The Syntactic Process.* MIT Press.
- Wadler, P. (2015). "Propositions as Types." *Communications of the ACM*, 58(12), 75--84.
- Wang, W., Juttler, B., Zheng, D., & Liu, Y. (2008). "Computation of rotation minimizing frames." *ACM Transactions on Graphics*, 27(1), 1--18.
- ISO/IEC 18004:2015. "Automatic identification and data capture techniques --- QR Code bar code symbology specification."
- Bitcoin Improvement Proposal 39 (BIP39). "Mnemonic code for generating deterministic keys."
