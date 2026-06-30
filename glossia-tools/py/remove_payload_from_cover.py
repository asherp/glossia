"""
Remove payload words from cover_ngram.yaml by commenting them out.

Reads payload.yaml to get the set of payload words, then processes
cover_ngram.yaml line by line, commenting out any word entry (and its
POS sub-entries) that appears in the payload.
"""

import yaml
import sys
import os


def get_payload_words(payload_path: str) -> set:
    """Load all top-level keys from payload.yaml."""
    with open(payload_path, "r") as f:
        data = yaml.safe_load(f)
    return set(data.keys())


def comment_out_payload_words(cover_path: str, payload_words: set, output_path: str = None):
    """
    Read cover_ngram.yaml and comment out entries whose word is in payload_words.

    A word entry starts with a line like 'word:' at column 0, followed by
    indented POS lines like '  Adj: 0.5'. We comment out the word line and
    all its indented sub-lines.
    """
    if output_path is None:
        output_path = cover_path

    with open(cover_path, "r") as f:
        lines = f.readlines()

    out_lines = []
    commenting = False
    commented_count = 0

    for line in lines:
        stripped = line.rstrip("\n")

        # Already a comment or blank line — pass through, but stop commenting mode
        if stripped == "" or stripped.lstrip().startswith("#"):
            out_lines.append(line)
            commenting = False
            continue

        # Check if this is a top-level key (no leading whitespace, ends with ':')
        if not line[0].isspace() and ":" in stripped:
            word = stripped.split(":")[0]
            if word in payload_words:
                commenting = True
                commented_count += 1
                out_lines.append("# " + line)
                continue
            else:
                commenting = False
                out_lines.append(line)
                continue

        # Indented line — belongs to previous top-level key
        if commenting:
            out_lines.append("# " + line)
        else:
            out_lines.append(line)

    with open(output_path, "w") as f:
        f.writelines(out_lines)

    return commented_count


def main():
    base_dir = os.path.dirname(os.path.abspath(__file__))
    payload_path = os.path.join(base_dir, "languages", "english", "payload.yaml")
    cover_path = os.path.join(base_dir, "languages", "english", "cover_ngram.yaml")

    print(f"Loading payload words from {payload_path}...")
    payload_words = get_payload_words(payload_path)
    print(f"  Found {len(payload_words)} payload words.")

    print(f"Processing {cover_path}...")
    count = comment_out_payload_words(cover_path, payload_words)
    print(f"  Commented out {count} word entries from cover_ngram.yaml.")
    print("Done. File updated in place.")


if __name__ == "__main__":
    main()
