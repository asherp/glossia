#!/usr/bin/env python3
"""Build prosody data (syllables, stress, rhyme) for a Glossia wordlist.

Reads the real wordlists under `languages/<lang>/` and emits a single
`prosody.yaml`-shaped file plus a coverage/distribution report. Pronunciations
come from CMUdict; words CMUdict does not know fall back to a vowel-group
syllable heuristic and are marked `heuristic` so the gap is auditable (same
`source:` discipline as the semantic planner's WordNet data).

Shape of the emitted file:

    stress: {abandon: "010", record: "010|100", ...}
    rhyme:  {abandon: "AENDAHN", ...}
    heuristic: [word, ...]   # words with no CMUdict entry

A stress string is one digit per syllable (CMUdict's 0/1/2), so its LENGTH is
the syllable count — one map carries both facts. `|` separates pronunciation
variants that differ in stress or syllable count; a word with variants is
genuinely flexible for meter ("record" is both RE-cord and re-CORD), which is
free slack the fitter can spend.

Usage: python3 build_prosody.py [language] [wordlist-profile]
       (default: english bip39 — payload_bip39.yaml + cover.yaml)
"""
import sys, os, re, collections
import yaml
import cmudict

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

VOWEL_RE = re.compile(r"[AEIOU]")


def variants(prons):
    """Distinct (stress-pattern, rhyme-key) pairs for a word's pronunciations."""
    out = []
    for p in prons:
        stress = "".join(ph[-1] for ph in p if ph[-1].isdigit())
        if not stress:
            continue
        rhyme = rhyme_key(p)
        out.append((stress, rhyme))
    # dedupe, preserve order (CMUdict lists the primary pronunciation first)
    seen, uniq = set(), []
    for v in out:
        if v not in seen:
            seen.add(v)
            uniq.append(v)
    return uniq


def rhyme_key(pron):
    """Perfect-rhyme key: last primary-stressed vowel through the end, destressed.

    Falls back to the last vowel when no syllable carries primary stress, so
    every pronounceable word gets a key.
    """
    idx = [i for i, ph in enumerate(pron) if ph[-1].isdigit()]
    if not idx:
        return ""
    start = next((i for i in idx if pron[i].endswith("1")), idx[-1])
    return "".join(re.sub(r"\d", "", ph) for ph in pron[start:])


def heuristic_syllables(word):
    """Vowel-group count with a silent-final-e correction. Deliberately crude —
    it exists to measure how often we need it, not to be right."""
    w = word.lower()
    groups = re.findall(r"[aeiouy]+", w)
    n = len(groups)
    if w.endswith("e") and n > 1 and not w.endswith(("le", "ee", "ye")):
        n -= 1
    return max(1, n)


def load_wordlist(path):
    # BaseLoader keeps every key a string: plain YAML would turn the words
    # "no", "on", "off" into booleans and lose them.
    with open(path) as f:
        return yaml.load(f, Loader=yaml.BaseLoader)


def build(words, cmu):
    stress, rhyme, heur = {}, {}, []
    for w in words:
        prons = cmu.get(w.lower())
        vs = variants(prons) if prons else []
        if vs:
            stress[w] = "|".join(dict.fromkeys(v[0] for v in vs))
            rhyme[w] = vs[0][1]
        else:
            n = heuristic_syllables(w)
            # Unknown stress: '?' per syllable. The fitter reads '?' as "either",
            # so a heuristic word never blocks a line, it just carries no evidence.
            stress[w] = "?" * n
            rhyme[w] = ""
            heur.append(w)
    return stress, rhyme, heur


def report(name, words, stress, heur):
    print(f"\n=== {name}: {len(words)} words ===")
    print(f"  cmudict coverage : {len(words) - len(heur)}/{len(words)} "
          f"({100 * (len(words) - len(heur)) / len(words):.1f}%)")
    syl = collections.Counter()
    multi = 0
    for w in words:
        pats = stress[w].split("|")
        syl[len(pats[0])] += 1
        if len(pats) > 1:
            multi += 1
    total = sum(syl.values())
    print("  syllable distribution:")
    for n in sorted(syl):
        print(f"    {n} syl: {syl[n]:6d}  ({100 * syl[n] / total:5.1f}%)  "
              f"{'#' * int(60 * syl[n] / total)}")
    mean = sum(n * c for n, c in syl.items()) / total
    print(f"  mean syllables/word: {mean:.3f}")
    print(f"  words with >1 stress variant: {multi} ({100 * multi / total:.1f}%)")
    # Which stress patterns dominate — this is what a meter fitter has to work with.
    pat = collections.Counter(stress[w].split("|")[0] for w in words)
    print("  top stress patterns:")
    for p, c in pat.most_common(8):
        print(f"    {p:<6} {c:6d}  ({100 * c / total:5.1f}%)")
    return mean


def main():
    lang = sys.argv[1] if len(sys.argv) > 1 else "english"
    profile = sys.argv[2] if len(sys.argv) > 2 else "bip39"
    ldir = os.path.join(ROOT, "languages", lang)
    pfile = os.path.join(ldir, f"payload_{profile}.yaml" if profile != "default" else "payload.yaml")
    cfile = os.path.join(ldir, "cover.yaml")

    cmu = cmudict.dict()
    payload = sorted(load_wordlist(pfile))
    cover = sorted(k for k in load_wordlist(cfile))

    p_stress, p_rhyme, p_heur = build(payload, cmu)
    c_stress, c_rhyme, c_heur = build(cover, cmu)

    p_mean = report(f"payload ({os.path.basename(pfile)})", payload, p_stress, p_heur)
    c_mean = report(f"cover ({os.path.basename(cfile)})", cover, c_stress, c_heur)
    if p_heur:
        print(f"\n  payload words missing from cmudict: {' '.join(p_heur[:40])}"
              f"{' ...' if len(p_heur) > 40 else ''}")
    if c_heur:
        print(f"  cover words missing from cmudict: {' '.join(c_heur[:40])}"
              f"{' ...' if len(c_heur) > 40 else ''}")

    stress = dict(sorted({**p_stress, **c_stress}.items()))
    rhyme = dict(sorted({**p_rhyme, **c_rhyme}.items()))
    heur = sorted(set(p_heur) | set(c_heur))

    out = os.path.join(os.path.dirname(os.path.abspath(__file__)), "data",
                       f"prosody_{lang}_{profile}.yaml")
    header = (
        f"# Prosody data for {lang} ({profile} payload + cover), generated by\n"
        f"# experiments/prosody/build_prosody.py from CMUdict.\n"
        "#\n"
        "# stress:    word -> one digit per syllable (CMUdict 0/1/2), '|' between\n"
        "#            pronunciation variants. Length = syllable count. '?' = a\n"
        "#            syllable whose stress is unknown (heuristic fallback).\n"
        "# rhyme:     word -> perfect-rhyme key (last primary-stressed vowel to end,\n"
        "#            destressed). Empty when unknown.\n"
        "# heuristic: words with no CMUdict entry, listed for audit.\n"
        "#\n"
        f"# payload mean syllables/word: {p_mean:.3f}\n"
        f"# cover   mean syllables/word: {c_mean:.3f}\n"
    )
    with open(out, "w") as f:
        f.write(header)
        yaml.safe_dump({"stress": stress, "rhyme": rhyme, "heuristic": heur},
                       f, default_flow_style=True, width=100, sort_keys=True)
    print(f"\nwrote {os.path.relpath(out, ROOT)} "
          f"({len(stress)} words, {os.path.getsize(out) / 1024:.0f} KB)")


if __name__ == "__main__":
    main()
