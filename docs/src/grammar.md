# Grammar

Glossia uses a two-layer grammar system: a **context-free grammar (CFG)** layer that generates POS tag sequences, and a **type-driven layer** based on Montague Grammar that constrains POS slots via lambda calculus.

## Grammar Files

Grammars are defined in YAML files located in `languages/{language}/grammar.yaml`. Each grammar file specifies:

1. **Types**: Montague Grammar semantic types for each POS tag
2. **Rules**: CFG production rules with weights and lambda terms

### Example Grammar (English)

```yaml
grammar:
  name: "English Grammar"

  types:
    - name: "N"
      lambda_type: "e -> t"
    - name: "V"
      lambda_type: "e -> (e -> t)"
    - name: "Adj"
      lambda_type: "e -> e"
    - name: "Det"
      lambda_type: "(e -> t) -> (e -> t)"
    - name: "Cop"
      lambda_type: "(e -> t) -> (e -> t)"
    - name: "Dot"
      lambda_type: "t"

  rules:
    sentence:
      lambda: "N"
      cfg_productions:
        - production: "NP VP Dot"
          weight: 0.85
          lambda: "N"
        - production: "NP Adv V NP Dot"
          weight: 0.10
          lambda: "N"
        - production: "NP V NP Dot"
          weight: 0.05
          lambda: "N"

    noun_phrase:
      lambda: "N"
      cfg_productions:
        - production: "N"
          weight: 1.0
          lambda: "N"
        - production: "Det[def] N"
          weight: 0.50
          lambda: "Det"
        - production: "Det[indef] N"
          weight: 0.25
          lambda: "Det"

    verb_phrase:
      lambda: "N"
      cfg_productions:
        - production: "VP_BASE"
          weight: 0.70
          lambda: "N"
        - production: "VP_BASE VP_PP_TAIL"
          weight: 0.30
          lambda: "N"
```

### Key Syntax

- **Terminal symbols** are POS tags: `Det`, `Adj`, `N`, `V`, `Modal`, `Aux`, `Cop`, `To`, `Prep`, `Adv`, `Conj`, `Dot`, `Prefix`
- **Refinement tags** in brackets constrain cover word selection: `Det[def]` selects definite determiners ("the"), `Det[indef]` selects indefinite ("a", "an"), `Det[quant]` selects quantifiers ("each", "some")
- **Weights** determine probabilistic selection between alternative productions
- **Rule names** map to standard abbreviations: `noun_phrase` -> `NP`, `verb_phrase` -> `VP`, `sentence` -> `S`

## POS Tags

Glossia uses 13 parts of speech:

| POS | Description | Example Words |
|-----|-------------|---------------|
| `Det` | Determiner | the, a, each, some |
| `Adj` | Adjective | big, red, fast |
| `N` | Noun | cat, house, time |
| `V` | Verb | run, see, get |
| `Modal` | Modal verb | may, can, might |
| `Aux` | Auxiliary verb | has, will, does |
| `Cop` | Copula | is, are, was |
| `To` | Infinitive marker | to |
| `Prep` | Preposition | in, on, for, to |
| `Adv` | Adverb | very, already, out |
| `Conj` | Conjunction | and, but, or |
| `Dot` | Sentence terminator | . |
| `Prefix` | Sentence prefix | Re:, Fwd: |

### POS Slot Constraints

- **Payload-eligible POS**: `N`, `V`, `Adj`, `Adv`, `Prep`, `Det`, `Conj` - these slots can be filled with either payload or cover words
- **Cover-only POS**: `Aux`, `Cop`, `To`, `Prefix`, `Dot`, `Modal` - these slots are always filled with cover (function) words

## Montague Grammar Type System

Each POS tag maps to a semantic type from Montague Grammar:

| POS | Type | Interpretation |
|-----|------|----------------|
| N | `e -> t` | Predicate over entities |
| V | `e -> (e -> t)` | Relation between entities |
| Adj | `e -> e` | Entity modifier |
| Adv | `(e -> t) -> (e -> t)` | Predicate modifier |
| Det | `(e -> t) -> (e -> t)` | Quantifier |
| Prep | `e -> (e -> t)` | Prepositional relation |
| Conj | `t -> (t -> t)` | Proposition connective |
| Cop | `(e -> t) -> (e -> t)` | Subject-predicate linker |
| Modal/Aux/To | `(e -> t) -> (e -> t)` | Predicate modifier |
| Dot | `t` | Sentence truth value |
| Prefix | `t -> t` | Sentence wrapper |

Where:
- `e` = Entity type
- `t` = Truth value type
- `->` = Function type

These types ensure that constituents combine in semantically valid ways, beyond what the CFG alone can enforce.

## Dialect Support

Grammar files can define **dialect overlays** that modify the base rules for different generation modes. Dialects are defined in a separate `dialect.yaml` file:

```yaml
dialects:
  subject:
    rules:
      sentence:
        cfg_productions:
          - production: "Prefix NP Cop Adj Dot"
            weight: 0.30
          - production: "NP Cop Adj Dot"
            weight: 0.70

  poetry:
    rules:
      sentence:
        cfg_productions:
          - production: "NP VP Dot"
            weight: 0.50
          - production: "Adj N V Adv Dot"
            weight: 0.50
```

The `--grammar subject` and `--grammar body` flags select which dialect overlay to apply on top of the base grammar rules.

## Sentence Length Strategy

The grammar interacts with the length mode to control output:

- **Compact mode**: Tries sentence lengths from `k_min` to `k_max`, stopping at the first length where the payload fits. Within that length, picks the highest-probability POS sequence.
- **Natural mode**: Samples sentence length from the grammar's probability distribution across all valid lengths. This produces more varied, natural-sounding text.

The scoring function for balancing compactness vs naturalness:

```
score = log P(sequence) - λ * length
```

Higher λ yields shorter text; lower λ yields more natural text.
