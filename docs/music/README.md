# Glossia Music Dialect

Encode binary payloads as MIDI note sequences with musical structure.

## Encoding Capacity

- **Payload**: 128 chromatic notes (C-1 to G9) → **7 bits/note**
- **256-bit BIP39 seed**: 37 notes minimum (~10 bars @ 4 beats/bar)
- **Payload density**: 85-90% (10-15% rests for musical breathing)

## Dialects

### `raw` - Maximum Density
Pure note sequences, no structural framing.

```bash
cargo run --bin glossia -- --from-ascii "Hello" --language music --dialect raw
```

**Output:**
```
A6 db9 f5 g1 g1
```

### `scored` - Musical Structure
Headers, barlines, dynamics, articulations for listenable output.

```bash
cargo run --bin glossia -- --from-ascii "Hello" --language music --dialect scored
```

**Output:**
```
tempo=120 time=4/4
p a6 half db9 whole f5 half g1 whole |
||
```

## Text Notation Format

The primary output format is **space-separated text tokens**:

| Token Type | Examples | POS Tag | Purpose |
|------------|----------|---------|---------|
| **Payload notes** | `C4`, `Eb3`, `Gb5` | N | Carry data (7 bits each) |
| **Durations** | `whole`, `half`, `quarter`, `eighth`, `dotted-half` | Adj | Modify note length |
| **Dynamics** | `pp`, `p`, `mp`, `mf`, `f`, `ff`, `sfz` | Adv | Volume expression |
| **Articulations** | `legato`, `staccato`, `tenuto`, `marcato`, `accent` | Modal | Attack/release style |
| **Barlines** | `\|`, `\|\|` | Dot | Structural markers |
| **Rests** | `rest` | Cop | Silent beats |
| **Ties** | `tie` | To | Connect notes |

## MIDI Binary Rendering

Convert text notation to `.mid` files using the included Python script:

```bash
# Install dependency
pip3 install midiutil

# Convert to MIDI
./languages/music/render_midi.py input.txt output.mid

# With custom tempo
./languages/music/render_midi.py input.txt output.mid --tempo 140

# From stdin (raw dialect)
cargo run --bin glossia -- --from-ascii "test" --language music --dialect raw | \
  ./languages/music/render_midi.py - test.mid

# From stdin (scored dialect)
cargo run --bin glossia -- --from-ascii "Hello, World!" --language music --dialect scored | \
  ./languages/music/render_midi.py - hello.mid --verbose
```

## Round-Trip Encoding/Decoding

Encoding and decoding work identically to other Glossia languages:

```bash
# Encode
echo "test" | cargo run --bin glossia -- --from-ascii - --language music --dialect raw
# Output: A6 g9 f5 g8 g9

# Decode
echo "A6 g9 f5 g8 g9" | cargo run --bin glossia -- --decode --language music
# Output: test
```

**Note**: The decoder extracts only payload notes (N-tagged tokens) and ignores all cover tokens (dynamics, durations, barlines, etc.). Both `raw` and `scored` dialects decode identically.

## Architecture

### Payload (`payload.yaml`)
128 MIDI notes in scientific pitch notation, all tagged `N: 1.0`:
- Chromatic scale: C, Db, D, Eb, E, F, Gb, G, Ab, A, Bb, B
- Octave range: -1 to 9 (MIDI 0-127)
- Middle C = C4 (MIDI 60)

### Cover (`cover.yaml`)
Structural tokens using function-word POS tags only:
- **Dot**: Barlines (`|`, `||`)
- **Adj**: Durations (whole, half, dotted-half, quarter, dotted-quarter, eighth, sixteenth)
- **Adv**: Dynamics (pp, p, mp, mf, f, ff, sfz)
- **Modal**: Articulations (legato, staccato, tenuto, marcato, accent)
- **Cop**: Rests
- **Aux**: Headers (tempo, time, key)
- **Conj**: Separators (`\n`)
- **To**: Ties

### Grammar (`grammar.yaml`)
Context-free grammar with Montague semantic types:
- **Base rules**: 4/4 time, weighted note/rest/dynamic choices
- **Raw dialect**: Dense sequences (`N N N...`)
- **Scored dialect**: Headers + variable bar lengths (expressive timing)

## Design Principles

1. **Text-first output**: Accessible, debuggable, screen-reader friendly
2. **Payload/cover separation**: Clean decoding via POS tag filtering
3. **Musical coherence**: Dynamics and articulations for listenability
4. **Configurable density**: Balance between capacity and musicality via grammar weights

## Future Enhancements (v2)

- [ ] **Phrase structure**: Cadence patterns (V-I, IV-V-I) for longer compositions
- [ ] **Subdivisions**: Multiple notes per beat (eighth/sixteenth runs)
- [ ] **Chord notation**: Parallel N slots for harmonic richness
- [ ] **Scale dialects**: Diatonic, pentatonic, blues scale filtering
- [ ] **Header rendering**: Template interpretation (`Aux[tempo]` → `tempo=120`)
- [ ] **Style dialects**: Jazz, classical, minimalist via weighted grammar variations

## Examples

### Encode a BIP39 seed phrase as music
```bash
# Generate 12-word BIP39 mnemonic
cargo run --bin glossia -- --random 12 --language english > seed.txt

# Encode as music
cat seed.txt | cargo run --bin glossia -- --from english --into music > music.txt

# Convert to MIDI
./languages/music/render_midi.py music.txt seed.mid --tempo 100
```

### Encode arbitrary data
```bash
# Encode a file as music
xxd -p secret.txt | tr -d '\n' | \
  cargo run --bin glossia -- --from hex --into music --dialect scored > encoded.txt

# Play it back (requires MIDI player)
./languages/music/render_midi.py encoded.txt encoded.mid
fluidsynth -a alsa -i soundfont.sf2 encoded.mid
```

## Accessibility

The music dialect is designed with visually impaired users in mind:

1. **Text-first**: Screen readers can verify note sequences
2. **Auditory playback**: MIDI rendering enables listening to encoded data
3. **Slow playback**: Adjust tempo for verification (`--tempo 60`)
4. **Interval recognition**: Musical training aids in error detection
5. **Haptic feedback**: Future integration with haptic MIDI devices

## Credits

Architecture inspired by:
- **Solresol** (musical language by François Sudre)
- **MIDI standard** (Musical Instrument Digital Interface)
- **Montague Grammar** (compositional semantics)
- **BIP39** (mnemonic encoding for cryptocurrency)
