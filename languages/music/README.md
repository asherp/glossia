# Music Dialect

Encode binary payloads as MIDI note sequences.

## Quick Start

```bash
# Encode text as music
echo "Hello" | cargo run --bin glossia -- --from-ascii - --language music --dialect raw

# Decode back
echo "A6 db9 f5 g1 g1" | cargo run --bin glossia -- --decode --language music
```

## Documentation

- **[Complete Guide](../../docs/music/README.md)** - Full reference documentation
- **[Examples & Demo](../../docs/music/DEMO.md)** - Working examples and tutorials
- **[Scale Design](../../docs/music/SCALES_AND_STYLES.md)** - Future scale dialects and musical styles

## Files

- `payload.yaml` - 128 MIDI notes (C-1 to G9), 7 bits/note
- `cover.yaml` - Structural tokens (dynamics, durations, barlines)
- `grammar.yaml` - CFG with raw and scored dialects
- `render_midi.py` - Convert text notation to .mid files

## MIDI Rendering

```bash
# Install dependency
pip3 install midiutil

# Render to MIDI
./render_midi.py input.txt output.mid --tempo 120
```

See [docs/music/](../../docs/music/) for complete documentation.
