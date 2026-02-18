# Music Dialect Demo

Complete demonstration of the Glossia music dialect from encoding to MIDI playback.

## Installation

```bash
# Build Glossia
cd /path/to/glossia
cargo build --release

# Install MIDI rendering dependency
pip3 install midiutil
```

## Example 1: Simple Text Encoding (Raw Dialect)

```bash
# Encode a message
echo "Hello, Glossia!" | cargo run --bin glossia -- \
  --from-ascii - --language music --dialect raw
```

**Output:**
```
A6 db9 f5 g1 g1 g4 c3 f bb1 Eb2 g4 db9 db9 f4 f g7 ab1
```

**Decode back:**
```bash
echo "A6 db9 f5 g1 g1 g4 c3 f bb1 Eb2 g4 db9 db9 f4 f g7 ab1" | \
  cargo run --bin glossia -- --decode --language music
```

**Output:**
```
Hello, Glossia!
```

## Example 2: Musical Structure (Scored Dialect)

```bash
# Encode with musical framing
echo "Music is data" | cargo run --bin glossia -- \
  --from-ascii - --language music --dialect scored --seed 123
```

**Output:**
```
tempo=120 time=4/4
mf C4 quarter Eb5 half G6 quarter B3 whole |
F4 dotted-half A5 quarter rest half D6 eighth |
||
```

## Example 3: Generate MIDI File

```bash
# Encode → Convert to MIDI
echo "Test 123" | cargo run --bin glossia -- \
  --from-ascii - --language music --dialect scored --seed 42 \
  > test.txt

./languages/music/render_midi.py test.txt test.mid --tempo 100 --verbose
```

**Output:**
```
Parsed 7 note events:
  1. A6 @ t=0.00 dur=1.80 vel=50
  2. Db9 @ t=2.00 dur=3.60 vel=50
  3. F5 @ t=6.00 dur=1.80 vel=50
  4. G1 @ t=8.00 dur=3.60 vel=50
  5. G1 @ t=12.00 dur=1.80 vel=50
  6. G4 @ t=14.00 dur=0.45 vel=50
  7. Bb2 @ t=14.50 dur=1.80 vel=50
✓ Wrote 7 notes to test.mid (tempo=100 BPM)
```

**Play the MIDI file:**
```bash
# macOS
open test.mid

# Linux with fluidsynth
fluidsynth -a alsa -i soundfont.sf2 test.mid

# Or import into DAW (Logic, Ableton, FL Studio, etc.)
```

## Example 4: BIP39 Seed as Music

```bash
# Generate a 12-word BIP39 mnemonic
cargo run --bin glossia -- --random 12 --language english > seed.txt
cat seed.txt
# Output: abandon ability able about above absent absorb abstract absurd abuse access accident

# Encode as music
cat seed.txt | cargo run --bin glossia -- \
  --from english --into music --dialect scored --seed 1 \
  > seed_music.txt

# Convert to MIDI
./languages/music/render_midi.py seed_music.txt seed.mid --tempo 80

# Verify round-trip
cat seed_music.txt | cargo run --bin glossia -- \
  --from music --into english
# Output: abandon ability able about above absent absorb abstract absurd abuse access accident
```

## Example 5: Cross-Language Pipeline

```bash
# Latin → Music → MIDI
echo "Lumos Maxima!" | \
  cargo run --bin glossia -- --from-ascii - --into latin --dialect prose --seed 10 | \
  cargo run --bin glossia -- --from latin --into music --dialect scored | \
  tee latin_spell.txt | \
  python3 languages/music/render_midi.py - latin_spell.mid --tempo 120

# Decode back to verify
cat latin_spell.txt | cargo run --bin glossia -- \
  --from music | cargo run --bin glossia -- --from hex
# Output: Lumos Maxima!
```

## Example 6: Encoding Capacity Test

```bash
# How many notes for a 256-bit seed?
head -c 32 /dev/urandom | xxd -p -c 32 > random_seed.hex
cat random_seed.hex

# Encode as music
cat random_seed.hex | cargo run --bin glossia -- \
  --from hex --into music --dialect raw > random_music.txt

# Count notes
wc -w random_music.txt
# Output: ~37 notes (256 bits / 7 bits per note = 36.57)

# Verify decoding
cat random_music.txt | cargo run --bin glossia -- \
  --from music --into hex
# Should match random_seed.hex
```

## Example 7: Musical Texture Variations

```bash
# Same data, different seeds → different musical textures
echo "The same payload" > payload.txt

# Version 1
cat payload.txt | cargo run --bin glossia -- \
  --from-ascii - --into music --dialect scored --seed 1 > v1.txt

# Version 2
cat payload.txt | cargo run --bin glossia -- \
  --from-ascii - --into music --dialect scored --seed 2 > v2.txt

# Version 3
cat payload.txt | cargo run --bin glossia -- \
  --from-ascii - --into music --dialect scored --seed 3 > v3.txt

# Convert all to MIDI
./languages/music/render_midi.py v1.txt v1.mid
./languages/music/render_midi.py v2.txt v2.mid
./languages/music/render_midi.py v3.txt v3.mid

# Listen to the variations - same data, different music!
```

## Expected Output Characteristics

### Raw Dialect
- **Density**: Maximum (no cover tokens except note names)
- **Sound**: Dense chromatic sequences, 12-tone serialist aesthetic
- **Use case**: Compact encoding, computer-to-computer transfer

### Scored Dialect
- **Density**: ~85-90% (includes rests, dynamics, articulations)
- **Sound**: Structured phrases with musical breathing, still chromatic but more listenable
- **Use case**: Human-verifiable encoding, auditory data sonification

## Decoding Performance

```bash
# Measure decoding speed
time cat random_music.txt | cargo run --release --bin glossia -- --from music --into hex
# Typical: <100ms for 37 notes
```

## Integration Examples

### With Nostr (NIP-04)
```bash
# Encrypt a Nostr DM as music
echo "Secret message" | cargo run --bin glossia -- \
  --from-ascii - --into cs --dialect nip04 | \
  cargo run --bin glossia -- --from cs --into music --dialect scored \
  > encrypted_music.mid
```

### With Image Encoding
```bash
# Encode an image payload as music
python3 languages/image/app.py encode small_image.png | \
  cargo run --bin glossia -- --from english --into music --dialect scored \
  > image_music.txt

./languages/music/render_midi.py image_music.txt image_music.mid
```

### With Math/Primes
```bash
# Encode prime factorization as music
echo "42" | cargo run --bin glossia -- \
  --from-ascii - --into math/primes | \
  cargo run --bin glossia -- --from math/primes --into music \
  > prime_music.txt
```

## Accessibility Workflow

For visually impaired users verifying data integrity:

```bash
# 1. Generate music from data
cat important_seed.txt | cargo run --bin glossia -- \
  --from english --into music --dialect scored > seed_audio.txt

# 2. Convert to MIDI at slow tempo for verification
./languages/music/render_midi.py seed_audio.txt seed_slow.mid --tempo 60

# 3. Listen to the MIDI file
# Each note represents 7 bits of data
# Musical patterns emerge from the data structure

# 4. Re-encode the same data with the same seed to verify
cat important_seed.txt | cargo run --bin glossia -- \
  --from english --into music --dialect scored --seed <SAME_SEED> \
  > seed_audio_verify.txt

# 5. Compare outputs
diff seed_audio.txt seed_audio_verify.txt
# Should be identical
```

## What This Sounds Like

- **Chromatic**: All 128 MIDI notes are used, so melodies span 10+ octaves with no tonal center
- **Atonal**: No key signature or scale constraints (by design - maximizes encoding capacity)
- **Varied rhythm**: Scored dialect adds rests, dynamics, and bar structure
- **Data-driven**: The payload determines the pitch sequence, not aesthetic choices

Think: **Stockhausen meets MIDI**, or **12-tone row meets data sonification**.

## Future: Scale-Filtered Dialects

Coming in v2 (see SCALES_AND_STYLES.md):
- **Pentatonic**: Pleasant, universal, ~5.5 bits/note
- **Blues**: Recognizable blues idiom, ~5.8 bits/note
- **Diatonic**: Familiar major/minor scales, ~6.0 bits/note

Trade-off: Lower capacity, higher musicality.

## Troubleshooting

**Q: MIDI file plays but sounds random/atonal?**
A: This is expected! The chromatic payload uses all 128 MIDI notes without harmonic constraints. For tonal music, use future scale-filtered dialects.

**Q: Decoding fails with "unknown word"?**
A: Make sure you're decoding the same dialect you encoded with. Raw and scored dialects decode identically (scored just adds cover tokens that are ignored).

**Q: MIDI renderer says "Parsed 0 note events"?**
A: Check that your input has valid note names (C4, Eb3, etc.). Raw dialect needs `--default-duration` flag since notes don't have explicit durations.

**Q: Notes span too many octaves (sounds weird)?**
A: The full chromatic range (C-1 to G9) is used for maximum capacity. Filter to a subset of octaves in post-processing, or wait for octave-constrained dialects in v2.

## Learn More

- `README.md` — Complete reference
- `SCALES_AND_STYLES.md` — Design docs for musical style dialects
- `../BUILD_INDEX.md` — How languages are registered
- `../../CLAUDE.md` — Glossia architecture overview
