# Music Pentatonic Dialect

Encode binary payloads as **pentatonic scale** note sequences — universally pleasant and accessible.

## Quick Start

```bash
# Encode text as pentatonic music
echo "Hello" | cargo run --bin glossia -- --from-ascii - --language music-pentatonic --dialect raw

# Decode back
cargo run --bin glossia -- --decode --language music-pentatonic < encoded.txt
```

## Encoding Capacity

- **Payload**: 45 notes (C, D, E, G, A across 9 octaves)
- **Capacity**: 5.49 bits/note (vs. 7 bits/note for chromatic)
- **256-bit payload**: ~47 notes (vs. 37 for chromatic)
- **Trade-off**: 27% less capacity, but **universally pleasant sound**

## Why Pentatonic?

The major pentatonic scale (C-D-E-G-A) is:
- **Universal**: Found in music worldwide (Chinese, African, Celtic, Blues, Native American)
- **Consonant**: No semitone intervals → naturally avoids dissonance
- **Accessible**: Easy to recognize and verify by ear
- **Memorable**: Creates melodic patterns that stick

## What It Sounds Like

Unlike the chromatic dialect (atonal, 12-tone), pentatonic produces:
- **Tonal music** with a clear key center (C major)
- **Melodic patterns** that sound intentional
- **Pleasant intervals** (no harsh dissonances)
- **Folk-like character** (similar to traditional melodies)

Think: **Chinese folk music**, **Scottish bagpipes**, **blues licks**, **gamelan**

## Comparison

| Feature | Chromatic | Pentatonic |
|---------|-----------|------------|
| Notes | 128 | 45 |
| Bits/note | 7.00 | 5.49 |
| 256-bit payload | 37 notes | 47 notes |
| Sound | Atonal, experimental | Tonal, pleasant |
| Accessibility | High (dense) | Higher (melodic) |
| Use case | Max capacity | Human-friendly |

## Examples

### Encode a Message
```bash
echo "Test 123" | cargo run --bin glossia -- \
  --from-ascii - --language music-pentatonic --dialect scored
```

**Output** (example):
```
tempo=120 time=4/4
C4 quarter E5 half G3 quarter A4 whole |
D4 half C5 quarter E4 eighth A3 half |
||
```

**Notice**: Only C, D, E, G, A notes appear (no flats/sharps)

### Convert to MIDI
```bash
echo "Hello, World!" | cargo run --bin glossia -- \
  --from-ascii - --language music-pentatonic --dialect scored \
  > penta_hello.txt

../music/render_midi.py penta_hello.txt penta_hello.mid --tempo 100
```

The resulting MIDI file sounds melodic and pleasant compared to chromatic.

### Cross-Dialect Translation
```bash
# Chromatic → Pentatonic (same data, different sound)
echo "Secret" | cargo run --bin glossia -- \
  --from-ascii - --into music --dialect raw > chromatic.txt

echo "Secret" | cargo run --bin glossia -- \
  --from-ascii - --into music-pentatonic --dialect raw > pentatonic.txt

# Both decode to "Secret", but sound very different
```

## Use Cases

1. **Accessibility**: Visually impaired users can verify data by listening to melodic patterns
2. **Education**: Teaching data encoding with pleasant, memorable music
3. **Steganography**: Hide data in music that sounds natural (not suspicious)
4. **Art**: Generate actual music from data (data sonification as aesthetic)
5. **Memory**: Use melodic patterns as mnemonic aids

## Shared Files

This dialect shares structural files with the base music dialect:
- `cover.yaml` → `../music/cover.yaml` (dynamics, articulations, durations)
- `grammar.yaml` → `../music/grammar.yaml` (raw and scored dialects)
- `payload.yaml` → pentatonic scale (45 notes)

## Documentation

See [docs/music/](../../docs/music/) for complete music dialect documentation.

## Future: Other Scales

Coming soon:
- **music-blues**: 6-note blues scale (5.75 bits/note)
- **music-diatonic**: 7-note major scale (5.98 bits/note)
- **music-minor**: Natural minor scale
- **music-dorian**: Modal scales for different moods
