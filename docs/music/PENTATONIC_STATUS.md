# Pentatonic Dialect - ✅ FULLY WORKING

## ✅ Complete Implementation

### Scale-Derived Wordlists

Pentatonic (and other scale-based dialects) are now **derived at runtime** from the
chromatic payload via interval patterns defined in `grammar.yaml`. No static
`payload_pentatonic.yaml` file is needed.

```yaml
# In languages/music/grammar.yaml
pentatonic:
  parent: raw
  scale:
    intervals: [2, 2, 3, 2, 3]   # major pentatonic
    root: C
```

The `scale:` definition specifies:
- **intervals**: semitone steps between consecutive scale degrees (must sum to 12)
- **root**: starting pitch class (C, D, Eb, F#, etc.)

At dialect load time, `DialectConfig` loads the full 128-note chromatic payload,
filters it to notes matching the scale's pitch classes, and injects the derived
wordlist into the in-memory cache. All downstream code (codec, pipeline, CLI)
sees a normal wordlist transparently.

### Available Scale Dialects

| Dialect | Intervals | Root | Notes/octave | Notes total | Bits/note |
|---------|-----------|------|-------------|-------------|-----------|
| `pentatonic` | [2,2,3,2,3] | C | 5 | ~54 | ~5.75 |
| `pentatonic-scored` | [2,2,3,2,3] | C | 5 | ~54 | ~5.75 |
| `minor-pentatonic` | [3,2,2,3,2] | A | 5 | ~54 | ~5.75 |
| `blues` | [3,2,1,1,3,2] | A | 6 | ~65 | ~6.02 |

### Adding New Scale Dialects

To add a new scale (e.g., whole-tone, diatonic), just add to `grammar.yaml`:

```yaml
whole-tone:
  parent: raw
  scale:
    intervals: [2, 2, 2, 2, 2, 2]
    root: C
```

To change the key, change the root:

```yaml
pentatonic-d:
  parent: scored
  scale:
    intervals: [2, 2, 3, 2, 3]
    root: D    # D major pentatonic: D, E, Gb, A, B
```

### Encoding / Decoding

```bash
# Encoding works via base-N codec (non-power-of-2 wordlist)
echo "Hello" | cargo run --bin glossia -- --from-ascii - --language music --dialect pentatonic

# Decoding
echo "a4 e6 a1 a2 g8 c3 g5 d1 e3" | cargo run --bin glossia -- --decode --language music --dialect pentatonic

# Full round-trip
echo "Hello, world!" | cargo run --bin glossia -- --from-ascii - --language music --dialect pentatonic | \
  cargo run --bin glossia -- --decode --language music --dialect pentatonic
```

## Architecture

### How Scale Derivation Works

1. `DialectConfig::from_language_dialect("music", "pentatonic")` parses `grammar.yaml`
2. Finds `scale: { intervals: [2,2,3,2,3], root: C }` in the pentatonic dialect
3. Loads the base chromatic payload (128 MIDI notes from `payload.yaml`)
4. Computes valid pitch classes: root C + intervals → {C(0), D(2), E(4), G(7), A(9)}
5. Filters chromatic payload to only notes with those pitch classes
6. Injects derived wordlist into the in-memory cache under key `"music:pentatonic"`
7. All subsequent `load_payload_words_for_wordlist("music", "pentatonic")` calls find it

### Key Modules

- `src/scale.rs` — pitch class mapping, interval pattern → pitch classes, payload filtering
- `src/grammar.rs` — `DialectConfig::parse_scale_ref()` parses scale definitions from YAML
- `src/generator/data.rs` — `inject_scale_payload()` populates the wordlist cache
- `languages/music/grammar.yaml` — declarative scale definitions per dialect

### Design Principle

A scale is defined by its **interval structure**, not a fixed set of pitches.
The grammar declares the structural rule (intervals + root), and the wordlist
is its extension — the set of all MIDI notes satisfying that predicate across
all octaves. This is the Montague Grammar approach: the scale is a predicate
`λroot. λnote. (note mod 12) ∈ intervals_from(root, pattern)`, and the payload
wordlist is its denotation.
