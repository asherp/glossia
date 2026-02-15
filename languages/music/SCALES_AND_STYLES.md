# Scales and Musical Styles as Dialects

## Overview

The current music dialect uses **all 128 chromatic notes** for maximum encoding capacity (7 bits/note). To create **musically constrained** dialects (blues, pentatonic, diatonic), we filter the payload to a subset of pitches.

## Trade-off: Capacity vs. Musicality

| Scale | Pitches | Bits/Note | 256-bit Payload Requires | Sound Quality |
|-------|---------|-----------|--------------------------|---------------|
| **Chromatic** (current) | 128 | 7.00 | 37 notes | Atonal, dense |
| **Diatonic** (7 notes × 9 octaves) | 63 | 5.98 | 43 notes | Familiar, tonal |
| **Pentatonic** (5 notes × 9 octaves) | 45 | 5.49 | 47 notes | Universal, pleasant |
| **Blues** (6 notes × 9 octaves) | 54 | 5.75 | 45 notes | Bluesy, expressive |
| **Whole Tone** (6 notes × 9 octaves) | 54 | 5.75 | 45 notes | Dreamy, ambiguous |
| **Octatonic** (8 notes × 9 octaves) | 72 | 6.17 | 42 notes | Jazz, Stravinsky |

## Implementation Strategy

### Option 1: Filtered Payload (Simple)
Create scale-specific `payload.yaml` files that subset the chromatic scale:

**`languages/music-blues/payload.yaml`**:
```yaml
# Blues scale: 1, ♭3, 4, ♯4/♭5, 5, ♭7
# Across 9 octaves (C0-C8)

# Octave 0
C0:
  N: 1.0
Eb0:
  N: 1.0
F0:
  N: 1.0
Gb0:
  N: 1.0
G0:
  N: 1.0
Bb0:
  N: 1.0

# Octave 1
C1:
  N: 1.0
Eb1:
  N: 1.0
F1:
  N: 1.0
# ... (repeat for all octaves)
```

**Pros**:
- Simple: just create new wordlists
- Automatic capacity calculation
- Clean separation

**Cons**:
- Duplicates the scale definition across octaves
- Less flexible (can't transpose)

### Option 2: Grammar-Driven Scale Constraint (Advanced)
Keep the full chromatic payload, but add grammar rules that enforce scale membership:

**`languages/music/grammar.yaml`** (new dialect):
```yaml
dialects:
  blues:
    scale: [0, 3, 5, 6, 7, 10]  # Blues scale intervals
    root: C  # Tonic
    rules:
      BEAT:
        cfg_productions:
          - production: "N[in-scale] Adj"
            weight: 0.90  # Strongly prefer in-scale notes
          - production: "N Adj"
            weight: 0.10  # Allow chromatic passing tones
```

**Pros**:
- Flexible: can transpose, modulate, use passing tones
- Richer musical expression
- Educational (shows scale structure)

**Cons**:
- Requires grammar engine extensions (scale-aware refinement tags)
- More complex implementation

### Option 3: Post-Processing Filter (Hybrid)
Generate chromatic output, then filter to nearest scale degree:

```bash
cargo run --bin glossia -- --from-ascii "test" --language music --dialect raw | \
  python3 filter_to_blues_scale.py > blues_test.txt
```

**Pros**:
- Works with existing dialect
- Easy to experiment

**Cons**:
- Lossy (payload notes get mapped to nearest scale degree)
- Decoding requires knowing the filter mapping

## Recommended Approach: Option 1 (Scale-Specific Dialects)

Create separate dialect directories for each scale:

```
languages/
  music/              # Chromatic (128 notes, 7 bits/note)
  music-pentatonic/   # Pentatonic (45 notes, 5.49 bits/note)
  music-blues/        # Blues (54 notes, 5.75 bits/note)
  music-diatonic-c/   # C major scale (63 notes, 5.98 bits/note)
  music-whole-tone/   # Whole tone (54 notes, 5.75 bits/note)
```

Each dialect inherits the grammar but uses a different `payload.yaml`.

## Musical Styles via Grammar Weights

Styles (jazz, classical, minimalist) are **grammar variations**, not payload changes:

### Jazz Style
- More syncopation (varied bar lengths)
- Extended harmonies (add 7ths, 9ths to chord slots in v2)
- Swing rhythm (triplet feel)

**`grammar.yaml` (jazz dialect)**:
```yaml
dialects:
  jazz:
    rules:
      BAR:
        cfg_productions:
          - production: "BEAT BEAT BEAT Dot"
            weight: 0.50  # More 3-beat "swing" bars
          - production: "BEAT BEAT BEAT BEAT Dot"
            weight: 0.30
          - production: "BEAT BEAT Dot"
            weight: 0.20
      BEAT:
        cfg_productions:
          - production: "Adv N Adj"
            weight: 0.40  # More dynamics (expressive)
          - production: "N Adj"
            weight: 0.40
          - production: "Modal N Adj"
            weight: 0.20  # More articulation
```

### Classical Style
- Regular phrases (strict 4/4)
- Clear dynamics and articulations
- Longer note values

**`grammar.yaml` (classical dialect)**:
```yaml
dialects:
  classical:
    rules:
      BAR:
        cfg_productions:
          - production: "BEAT BEAT BEAT BEAT Dot"
            weight: 1.0  # Strict 4/4
      BEAT:
        cfg_productions:
          - production: "N Adj"
            weight: 0.60
          - production: "Cop Adj"
            weight: 0.20  # More rests (breathing)
          - production: "Modal N Adj"
            weight: 0.15
          - production: "Adv N Adj"
            weight: 0.05
```

### Minimalist Style
- Repetitive patterns (low entropy)
- Sparse texture (more rests)
- Slow harmonic rhythm

**`grammar.yaml` (minimalist dialect)**:
```yaml
dialects:
  minimalist:
    rules:
      BEAT:
        cfg_productions:
          - production: "N Adj[whole]"
            weight: 0.40  # Long sustained notes
          - production: "Cop Adj[whole]"
            weight: 0.30  # Long rests
          - production: "N Adj[half]"
            weight: 0.20
          - production: "N Adj[quarter]"
            weight: 0.10
```

## Example: Blues Scale Dialect

Let's create a quick proof-of-concept:

### Step 1: Generate Blues Payload
```bash
# Create blues scale payload (6 notes × 9 octaves = 54 notes)
# This would be languages/music-blues/payload.yaml
```

### Step 2: Test Encoding
```bash
cargo run --bin glossia -- --from-ascii "Hello" --language music-blues --dialect scored
```

**Expected output** (all notes in C blues scale):
```
tempo=120 time=4/4
mf C4 quarter Eb4 half F4 quarter Gb4 eighth |
G4 half Bb4 quarter C5 half |
||
```

### Step 3: Compare Sound Quality
- **Chromatic** (current): Sounds like 12-tone serialism (Schoenberg)
- **Blues**: Sounds like a blues melody (recognizable idiom)
- **Pentatonic**: Sounds like folk music (globally familiar)

## Next Steps

1. **Implement pentatonic dialect first** (simplest, most universally pleasant)
2. **Add scale-aware grammar rules** for passing tones and chromaticism
3. **Create style dialects** (jazz, classical, minimalist) via grammar weights
4. **Add chord progression support** (v2 feature) for harmonic structure

Would you like me to implement the **pentatonic dialect** as a proof of concept?
