# Glossia

Glossia encodes binary data (BIP39 mnemonics, API keys, arbitrary payloads) into grammatically correct natural language. It combines context-free grammars with Montague Grammar (lambda calculus) so that payload words are hidden as grammatically valid constituents within natural-looking prose. Decoding is trivial: filter output against the payload wordlist.

## Architecture

- **CFG layer** (`*.cfg` files): weighted context-free grammar productions generate POS tag sequences
- **Type-driven layer** (`grammar.yaml` files): Montague-style semantic types constrain POS slots via lambda calculus
- **Generator** (`src/generator/`): embeds payload words into POS slots using maximum subsequence matching, fills remaining slots with cover words
- **Merkle system** (`src/merkle.rs`): append-only wordlist trees with merkleization for cryptographic proofs
- **Build system** (`build.rs`): embeds all `languages/` YAML at compile time; debug builds embed English only

## Key types

```
Pos: Det | Adj | N | V | Modal | Aux | Cop | To | Prep | Adv | Conj | Dot | Prefix
SemanticType: Entity (e) | Truth (t) | Function(A -> B) | Refined(refinement, base)
LambdaTerm: Variable | Constant(Pos) | Application(f, a) | Abstraction(var, type, body)
```

## POS-to-type mapping (Montague Grammar)

| POS | Type | Interpretation |
|-----|------|----------------|
| N | `e -> t` | predicate over entities |
| V | `e -> (e -> t)` | takes entity, returns predicate |
| Adj | `e -> e` | entity modifier |
| Adv | `(e -> t) -> (e -> t)` | predicate modifier |
| Det | `(e -> t) -> (e -> t)` | quantifier |
| Prep | `e -> (e -> t)` | prepositional modifier |
| Conj | `t -> (t -> t)` | proposition connective |
| Cop | `(e -> t) -> (e -> t)` | subject-predicate linker |
| Modal/Aux/To | `(e -> t) -> (e -> t)` | predicate modifier |
| Dot | `t` | sentence-level truth |

## Wordlist rules

- Wordlists are **strictly append-only** for backward compatibility
- Payload words and cover words must be disjoint sets
- Function-word POS slots (Aux, Cop, To, Prefix, Dot) are reserved for cover words only

## Agents

Eight specialist agents are available via `/agents`. Each has deep context on its domain and the relevant files. See `.claude/agents/` for definitions.

| Agent | Focus |
|-------|-------|
| `linguist` | Montague Grammar, lambda calculus, semantic types, cross-linguistic grammar |
| `language-designer` | Wordlist generation toolchain, CFG authoring, POS tagging, new language setup |
| `cryptographer` | Encoding/decoding correctness, wordlist security, Merkle proofs |
| `rust-engineer` | Generator engine, pest parsers, caching, build system, CLI |
| `test-engineer` | Test coverage, round-trip verification, regression tests, cross-language validation |
| `mathematician` | Prime encodings, Merkle arithmetic, constructive number systems |
| `physicist` | Entropy-conserving exact simulations via prime factorization arithmetic |
| `image-artist` | Visual encoding - compositional grammar over shapes, colors, positions to embed payloads in images |
