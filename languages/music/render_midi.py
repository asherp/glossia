#!/usr/bin/env python3
"""
MIDI Binary Renderer for Glossia Music Dialect

Converts text notation (space-separated tokens) to MIDI binary (.mid files).

Usage:
    ./render_midi.py input.txt output.mid
    echo "C4 quarter E4 eighth G4 half" | ./render_midi.py - output.mid

    # With tempo override:
    ./render_midi.py input.txt output.mid --tempo 140

    # With time signature override:
    ./render_midi.py input.txt output.mid --time-signature "3/4"

Dependencies:
    pip install midiutil
"""

import sys
import re
from typing import List, Tuple, Optional
from dataclasses import dataclass
from midiutil import MIDIFile

# ─────────────────────────────────────────────────────────────────
# MIDI Note Mapping (Scientific Pitch Notation → MIDI Number)
# ─────────────────────────────────────────────────────────────────

PITCH_CLASS = {
    'C': 0, 'Db': 1, 'D': 2, 'Eb': 3, 'E': 4, 'F': 5,
    'Gb': 6, 'G': 7, 'Ab': 8, 'A': 9, 'Bb': 10, 'B': 11
}

def note_to_midi(note_name: str) -> int:
    """
    Convert scientific pitch notation to MIDI note number.

    Examples:
        C4  → 60 (middle C)
        A0  → 21 (lowest piano key)
        C-1 → 0  (MIDI minimum)
        G9  → 127 (MIDI maximum)
    """
    # Handle negative octaves: "C-1" → pitch="C", octave=-1
    match = re.match(r'^([A-G]b?)(-?\d+)$', note_name)
    if not match:
        raise ValueError(f"Invalid note name: {note_name}")

    pitch_class = match.group(1)
    octave = int(match.group(2))

    if pitch_class not in PITCH_CLASS:
        raise ValueError(f"Invalid pitch class: {pitch_class}")

    # MIDI formula: (octave + 1) * 12 + pitch_class
    # Octave 0 starts at MIDI 12, octave -1 at MIDI 0
    midi_num = (octave + 1) * 12 + PITCH_CLASS[pitch_class]

    if not 0 <= midi_num <= 127:
        raise ValueError(f"MIDI number {midi_num} out of range for {note_name}")

    return midi_num


# ─────────────────────────────────────────────────────────────────
# Duration Mapping (Text → MIDI Ticks)
# ─────────────────────────────────────────────────────────────────

# Duration in quarter notes (1.0 = quarter note)
DURATIONS = {
    'whole': 4.0,
    'dotted-half': 3.0,
    'half': 2.0,
    'dotted-quarter': 1.5,
    'quarter': 1.0,
    'eighth': 0.5,
    'sixteenth': 0.25,
}

# ─────────────────────────────────────────────────────────────────
# Dynamics Mapping (Text → MIDI Velocity)
# ─────────────────────────────────────────────────────────────────

DYNAMICS = {
    'pp': 40,   # pianissimo
    'p': 50,    # piano
    'mp': 60,   # mezzo-piano
    'mf': 75,   # mezzo-forte
    'f': 90,    # forte
    'ff': 105,  # fortissimo
    'sfz': 115, # sforzando
}

DEFAULT_VELOCITY = 80  # Default if no dynamic specified


# ─────────────────────────────────────────────────────────────────
# Token Parser
# ─────────────────────────────────────────────────────────────────

@dataclass
class NoteEvent:
    """A single MIDI note event with all performance attributes."""
    pitch: int           # MIDI note number (0-127)
    duration: float      # In quarter notes
    velocity: int        # MIDI velocity (0-127)
    time: float          # Start time in quarter notes from beginning
    articulation: Optional[str] = None


class TextNotationParser:
    """Parse Glossia music text notation into MIDI events."""

    def __init__(self, tempo: int = 120, time_signature: str = "4/4", default_duration: float = 1.0):
        self.tempo = tempo
        self.time_signature = time_signature
        self.default_duration = default_duration  # For raw dialect (notes without durations)
        self.current_time = 0.0
        self.current_velocity = DEFAULT_VELOCITY
        self.events: List[NoteEvent] = []

    def parse(self, text: str) -> List[NoteEvent]:
        """Parse text notation into MIDI events."""
        tokens = self._tokenize(text)
        self._parse_tokens(tokens)
        return self.events

    def _tokenize(self, text: str) -> List[str]:
        """Tokenize input, preserving quoted strings."""
        # Remove header lines (tempo=120, time=4/4)
        text = re.sub(r'tempo=\d+', '', text)
        text = re.sub(r'time=\d+/\d+', '', text)

        # Split on whitespace, but preserve | and || as tokens
        tokens = text.split()
        return [t for t in tokens if t]  # Remove empty strings

    def _parse_tokens(self, tokens: List[str]):
        """Parse token stream into note events."""
        i = 0
        pending_note = None
        pending_dynamic = None
        pending_articulation = None
        is_rest = False

        while i < len(tokens):
            token = tokens[i]

            # Barlines (structural, ignored for MIDI rendering)
            if token in ('|', '||'):
                i += 1
                continue

            # Newlines (ignored)
            if token == '\n':
                i += 1
                continue

            # Header tokens (already processed, ignore)
            if token in ('tempo', 'time', 'key', 'Tempo', 'Time', 'Key'):
                i += 1
                continue

            # Dynamics (modify next note)
            if token in DYNAMICS:
                pending_dynamic = DYNAMICS[token]
                i += 1
                continue

            # Articulations (modify next note)
            if token in ('legato', 'staccato', 'tenuto', 'marcato', 'accent'):
                pending_articulation = token
                i += 1
                continue

            # Rests (mark that next duration is a rest)
            if token == 'rest':
                is_rest = True
                i += 1
                continue

            # Ties (connect notes, but we'll just continue the current note)
            if token == 'tie':
                # For simplicity, we'll treat tied notes as a single longer note
                # by not advancing time before the next note
                i += 1
                continue

            # Durations (apply to pending note or rest)
            if token in DURATIONS:
                duration = DURATIONS[token]

                if is_rest:
                    # Rest: just advance time
                    self.current_time += duration
                    is_rest = False
                elif pending_note is not None:
                    # Complete the pending note
                    velocity = pending_dynamic if pending_dynamic else self.current_velocity

                    # Apply articulation (affects duration)
                    if pending_articulation == 'staccato':
                        rendered_duration = duration * 0.5  # Shorten
                    elif pending_articulation == 'tenuto':
                        rendered_duration = duration * 1.0  # Full value
                    else:
                        rendered_duration = duration * 0.9  # Slight detachment

                    event = NoteEvent(
                        pitch=pending_note,
                        duration=rendered_duration,
                        velocity=velocity,
                        time=self.current_time,
                        articulation=pending_articulation
                    )
                    self.events.append(event)

                    # Update velocity for next note
                    if pending_dynamic:
                        self.current_velocity = pending_dynamic

                    # Advance time by duration
                    self.current_time += duration

                    # Reset pending state
                    pending_note = None
                    pending_dynamic = None
                    pending_articulation = None
                else:
                    # Duration without note (might be a rest or malformed)
                    # Just advance time
                    self.current_time += duration

                i += 1
                continue

            # MIDI note names (C4, Eb3, etc.)
            # Try to parse as note (case-insensitive for robustness)
            try:
                # Handle both lowercase and uppercase (a6 vs A6)
                normalized = token[0].upper() + token[1:].lower()
                # But keep the original case for flat/sharp (Db vs db)
                if len(token) > 1 and token[1] in ('b', 'B'):
                    normalized = token[0].upper() + 'b' + token[2:]
                midi_num = note_to_midi(normalized)

                # Check if there's a pending note that needs to be flushed
                # (raw dialect: consecutive notes without durations)
                if pending_note is not None:
                    # Flush the previous note with default duration
                    velocity = pending_dynamic if pending_dynamic else self.current_velocity
                    event = NoteEvent(
                        pitch=pending_note,
                        duration=self.default_duration * 0.9,
                        velocity=velocity,
                        time=self.current_time,
                        articulation=pending_articulation
                    )
                    self.events.append(event)
                    self.current_time += self.default_duration
                    pending_dynamic = None
                    pending_articulation = None

                pending_note = midi_num
                i += 1
                continue
            except ValueError:
                # Not a valid note, skip
                i += 1
                continue

        # Flush any remaining pending note (end of sequence)
        if pending_note is not None:
            velocity = pending_dynamic if pending_dynamic else self.current_velocity
            event = NoteEvent(
                pitch=pending_note,
                duration=self.default_duration * 0.9,
                velocity=velocity,
                time=self.current_time,
                articulation=pending_articulation
            )
            self.events.append(event)


# ─────────────────────────────────────────────────────────────────
# MIDI File Writer
# ─────────────────────────────────────────────────────────────────

def write_midi(events: List[NoteEvent], output_path: str, tempo: int = 120):
    """Write MIDI events to a .mid file."""
    # Create MIDI file (1 track, format 1)
    midi = MIDIFile(1)
    track = 0
    channel = 0

    # Set tempo
    midi.addTempo(track, 0, tempo)

    # Add all note events
    for event in events:
        midi.addNote(
            track=track,
            channel=channel,
            pitch=event.pitch,
            time=event.time,
            duration=event.duration,
            volume=event.velocity
        )

    # Write to file
    with open(output_path, 'wb') as f:
        midi.writeFile(f)


# ─────────────────────────────────────────────────────────────────
# CLI
# ─────────────────────────────────────────────────────────────────

def main():
    import argparse

    parser = argparse.ArgumentParser(
        description='Convert Glossia music text notation to MIDI binary',
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  %(prog)s input.txt output.mid
  %(prog)s input.txt output.mid --tempo 140
  %(prog)s input.txt output.mid --time-signature "3/4"
  echo "C4 quarter E4 eighth G4 half" | %(prog)s - output.mid
        """
    )

    parser.add_argument('input', help='Input text file (use - for stdin)')
    parser.add_argument('output', help='Output MIDI file (.mid)')
    parser.add_argument('--tempo', type=int, default=120,
                        help='Tempo in BPM (default: 120)')
    parser.add_argument('--time-signature', default='4/4',
                        help='Time signature (default: 4/4)')
    parser.add_argument('--default-duration', type=float, default=1.0,
                        help='Default duration in quarter notes for raw dialect (default: 1.0)')
    parser.add_argument('--verbose', '-v', action='store_true',
                        help='Show parsed events')

    args = parser.parse_args()

    # Read input
    if args.input == '-':
        text = sys.stdin.read()
    else:
        with open(args.input, 'r') as f:
            text = f.read()

    # Parse text notation
    parser_obj = TextNotationParser(
        tempo=args.tempo,
        time_signature=args.time_signature,
        default_duration=args.default_duration
    )
    events = parser_obj.parse(text)

    if args.verbose:
        print(f"Parsed {len(events)} note events:")
        for i, event in enumerate(events):
            note_name = _midi_to_note(event.pitch)
            print(f"  {i+1}. {note_name} @ t={event.time:.2f} dur={event.duration:.2f} vel={event.velocity}")

    # Write MIDI file
    write_midi(events, args.output, tempo=args.tempo)
    print(f"✓ Wrote {len(events)} notes to {args.output} (tempo={args.tempo} BPM)")


def _midi_to_note(midi_num: int) -> str:
    """Convert MIDI number back to note name (for debugging)."""
    octave = (midi_num // 12) - 1
    pitch_class = midi_num % 12
    note_names = ['C', 'Db', 'D', 'Eb', 'E', 'F', 'Gb', 'G', 'Ab', 'A', 'Bb', 'B']
    return f"{note_names[pitch_class]}{octave}"


if __name__ == '__main__':
    main()
