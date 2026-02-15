#!/usr/bin/env python3
from __future__ import annotations

import argparse
from pathlib import Path


def is_top_level_key(line: str) -> bool:
    if not line.strip():
        return False
    if line.lstrip().startswith("#"):
        return False
    if line[0].isspace():
        return False
    return line.rstrip().endswith(":")


def dedupe_payload(text: str) -> str:
    lines = text.splitlines(keepends=True)
    seen: set[str] = set()
    out_lines: list[str] = []
    skip = False

    for line in lines:
        if is_top_level_key(line):
            key = line.rstrip()[:-1]
            if key in seen:
                skip = True
                continue
            seen.add(key)
            skip = False
            out_lines.append(line)
            continue

        if skip:
            # Preserve top-level comments/blank lines while skipping dup blocks.
            if (not line.strip()) or (
                line.lstrip().startswith("#") and not line[0].isspace()
            ):
                out_lines.append(line)
            continue

        out_lines.append(line)

    return "".join(out_lines)


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Remove duplicate top-level keys from a payload.yaml file."
    )
    parser.add_argument(
        "input",
        nargs="?",
        default="languages/latin/payload.yaml",
        help="Path to payload.yaml (default: languages/latin/payload.yaml)",
    )
    parser.add_argument(
        "-o",
        "--output",
        default=None,
        help="Write output to this path (default: overwrite input file)",
    )
    args = parser.parse_args()

    input_path = Path(args.input)
    output_path = Path(args.output) if args.output else input_path

    text = input_path.read_text(encoding="utf-8")
    deduped = dedupe_payload(text)
    output_path.write_text(deduped, encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
