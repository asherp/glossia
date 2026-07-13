#!/usr/bin/env python3
"""
Author the REAL semantic data for the full BIP39 payload wordlist.

Strategy (the hybrid discussed in the design thread): WordNet gives a strong
first pass, an LLM-authored override layer fixes what WordNet gets wrong for our
purposes. Every classification records its `source` so a human can audit exactly
what was machine-guessed vs. hand-set.

Nouns:  WordNet lexname (noun.animal, noun.artifact, noun.person, ...) -> fine
        class in slash notation, then AGENTIVE / NOUN_OVERRIDE layers.
Verbs:  WordNet verb lexname (verb.cognition, verb.motion, ...) -> a small set of
        selectional-restriction ARCHETYPES (subject/object class expectations),
        then VERB_OVERRIDE.

Outputs (consumed by sweep_real.py and, eventually, the Rust generator):
  data/noun_classes_bip39.yaml
  data/verb_frames_bip39.yaml

Run:  python3 build_data.py
"""

import os
import yaml
from nltk.corpus import wordnet as wn

HERE = os.path.dirname(os.path.abspath(__file__))
BIP39 = os.path.join(HERE, "..", "..", "languages", "english", "payload_bip39.yaml")
OUT = os.path.join(HERE, "data")
os.makedirs(OUT, exist_ok=True)

# --------------------------------------------------------------------------- #
# Class system. Fine classes carry a top-level bucket via slash notation.
# The 4-class operating point (quad) is: animate / agentive / thing / abstract
# (place folds into thing). Tops: animate, agentive, thing, place, abstract.
# --------------------------------------------------------------------------- #
NOUN_LEXNAME_TO_CLASS = {
    "noun.person":        "animate/person",
    "noun.animal":        "animate/animal",
    "noun.artifact":      "thing/artifact",
    "noun.food":          "thing/food",
    "noun.substance":     "thing/substance",
    "noun.plant":         "thing/plant",
    "noun.body":          "thing/body",
    "noun.object":        "thing/object",      # natural objects: rock, hill, star
    "noun.possession":    "thing/artifact",    # money/property -> treat as thing
    "noun.location":      "place",
    "noun.group":         "abstract/group",
    "noun.cognition":     "abstract/info",
    "noun.communication": "abstract/info",
    "noun.act":           "abstract/event",
    "noun.event":         "abstract/event",
    "noun.process":       "abstract/event",
    "noun.phenomenon":    "abstract/event",
    "noun.attribute":     "abstract/concept",
    "noun.state":         "abstract/concept",
    "noun.feeling":       "abstract/concept",
    "noun.motive":        "abstract/concept",
    "noun.relation":      "abstract/concept",
    "noun.shape":         "abstract/concept",
    "noun.quantity":      "abstract/concept",
    "noun.time":          "abstract/concept",
    "noun.Tops":          "abstract/concept",
}

# Machines/devices that can be intentional-ish subjects ("the engine runs",
# "the sensor detects"). WordNet files these under noun.artifact; for verb
# selection they behave like weak agents, so we lift them to `agentive`.
# (Intersected with the real BIP39 list at build time.)
AGENTIVE_CANDIDATES = {
    "engine", "motor", "machine", "robot", "device", "rocket", "satellite",
    "drone", "camera", "radar", "pump", "turbine", "computer", "laptop",
    "phone", "car", "truck", "train", "boat", "ship",
    "vehicle", "system", "network", "sensor",
    # NOTE: `clock` deliberately excluded — it's our canonical inanimate example
    # ("clock discovers mountain"); agentive would wrongly license "clock detects".
}

# WordNet-picks-wrong-sense fixes for our payload purposes. Kept short and
# explicit; grow as spot-checks surface errors.
NOUN_OVERRIDE = {
    "monster":  "agentive/creature",
    "ghost":    "agentive/creature",
    "crowd":    "animate/person",     # collective of people
    "army":     "animate/person",
    "family":   "animate/person",
    "team":     "animate/person",
    "brand":    "abstract/concept",
    "picture":  "thing/artifact",
    "coin":     "thing/artifact",
    # animal words WordNet ranks as "fierce person" first:
    "tiger":    "animate/animal",
    "wolf":     "animate/animal",
    "snake":    "animate/animal",
    "fox":      "animate/animal",
    "hawk":     "animate/animal",
    "lion":     "animate/animal",
    "shark":    "animate/animal",
    # natural features WordNet files as noun.object/group but read as places:
    "forest":   "place",
    "mountain": "place",
    "desert":   "place",
    "valley":   "place",
    "canyon":   "place",
    "ocean":    "place",
    "island":   "place",
    # people-ish that WordNet groups:
    "enemy":    "animate/person",
    # abstract words WordNet/concreteness over-concretizes:
    "notion":   "abstract/concept",
    "matter":   "abstract/concept",
}

# --------------------------------------------------------------------------- #
# Verb archetypes: selectional restrictions in TOP-level classes.
#   subj/obj = list of allowed tops, or "any".  obj = None => usually intransitive.
# --------------------------------------------------------------------------- #
ARCHETYPES = {
    "mental":        {"subj": ["animate"],             "obj": ["abstract"]},
    "emotion":       {"subj": ["animate"],             "obj": "any"},
    "communication": {"subj": ["animate"],             "obj": ["abstract", "animate"]},
    "perception":    {"subj": ["animate", "agentive"], "obj": "any"},
    "motion":        {"subj": ["animate", "agentive"], "obj": ["place"]},
    "contact":       {"subj": ["animate", "agentive"], "obj": ["thing", "animate"]},
    "creation":      {"subj": ["animate", "agentive"], "obj": ["thing"]},
    "consumption":   {"subj": ["animate"],             "obj": ["thing"]},
    "social":        {"subj": ["animate"],             "obj": ["animate", "abstract"]},
    "possession":    {"subj": ["animate", "agentive"], "obj": ["thing"]},
    "body":          {"subj": ["animate"],             "obj": ["thing"]},
    "change":        {"subj": "any",                   "obj": "any"},
    "stative":       {"subj": "any",                   "obj": "any"},
}

VERB_LEXNAME_TO_ARCH = {
    "verb.cognition":     "mental",
    "verb.emotion":       "emotion",
    "verb.communication": "communication",
    "verb.perception":    "perception",
    "verb.motion":        "motion",
    "verb.contact":       "contact",
    "verb.creation":      "creation",
    "verb.consumption":   "consumption",
    "verb.competition":   "social",
    "verb.social":        "social",
    "verb.possession":    "possession",
    "verb.body":          "body",
    "verb.change":        "change",
    "verb.stative":       "stative",
    "verb.weather":       "stative",
}

VERB_OVERRIDE = {
    # verbs whose dominant WordNet sense mis-frames them for our purposes
    "process":  "perception",   # machine-friendly
    "detect":   "perception",
    "measure":  "perception",
    "scan":     "perception",
    "compute":  "perception",
}


# `person` is intentionally NOT in the preference set: many abstract/emotion
# words carry a "beloved person" sense (love, dear) that would wrongly concretize
# them. Person is assigned only when it's the dominant sense (fallback path).
CONCRETE = {"animal", "artifact", "food", "plant",
            "substance", "body", "object", "location"}


def most_common_lexname(word, pos):
    ss = wn.synsets(word, pos)
    return ss[0].lexname() if ss else None


def best_noun_lexname(word):
    """Sense pick tuned for how a payload noun reads in prose:
      1. a rank-1 animate sense wins (person/animal as the DOMINANT sense —
         fixes agent nouns like worker/guest that have lower concrete senses);
      2. otherwise favor a concrete synset among the top few over WordNet's
         most-frequent (often metaphorical/abstract) one — fixes table/forest;
      3. otherwise fall back to the most-common sense.
    Rank-1 gating keeps 'love' (feeling rank-1, person rank-2) abstract."""
    ss = wn.synsets(word, "n")
    if not ss:
        return None
    r1 = ss[0].lexname()
    if r1.split(".")[1] in ("person", "animal"):
        return r1
    for s in ss[:4]:                         # only "reasonably common" senses
        if s.lexname().split(".")[1] in CONCRETE:
            return s.lexname()
    return r1


def classify_noun(word):
    if word in NOUN_OVERRIDE:
        return NOUN_OVERRIDE[word], "override"
    if word in AGENTIVE:
        return "agentive/machine", "agentive"
    lex = best_noun_lexname(word)
    if lex and lex in NOUN_LEXNAME_TO_CLASS:
        return NOUN_LEXNAME_TO_CLASS[lex], "wordnet"
    return "thing/object", "default"     # concrete-thing fallback; flagged below


def frame_verb(word):
    if word in VERB_OVERRIDE:
        return VERB_OVERRIDE[word], "override"
    lex = most_common_lexname(word, "v")
    if lex and lex in VERB_LEXNAME_TO_ARCH:
        return VERB_LEXNAME_TO_ARCH[lex], "wordnet"
    return "change", "default"           # permissive fallback (subj/obj any)


COVER = os.path.join(HERE, "..", "..", "languages", "english", "cover.yaml")


def classify_wordlist(name, pos):
    """Classify one wordlist's nouns/verbs; write data files; return a report."""
    # Skip non-string keys: YAML parses bare words like no/yes/on/off as bools.
    # Those are function words, never content nouns/verbs we classify.
    ok = lambda w: isinstance(w, str) and isinstance(pos[w], dict)
    nouns = sorted(w for w in pos if ok(w) and "N" in pos[w])
    verbs = sorted(w for w in pos if ok(w) and "V" in pos[w])

    noun_out, src_count, class_count, defaults = {}, {}, {}, []
    for w in nouns:
        cls, src = classify_noun(w)
        noun_out[w] = {"class": cls, "source": src}
        src_count[src] = src_count.get(src, 0) + 1
        class_count[cls.split("/")[0]] = class_count.get(cls.split("/")[0], 0) + 1
        if src == "default":
            defaults.append(w)

    verb_out, vsrc_count, arch_count = {}, {}, {}
    for w in verbs:
        arch, src = frame_verb(w)
        verb_out[w] = {"archetype": arch, "source": src}
        vsrc_count[src] = vsrc_count.get(src, 0) + 1
        arch_count[arch] = arch_count.get(arch, 0) + 1

    with open(os.path.join(OUT, f"noun_classes_{name}.yaml"), "w") as f:
        f.write(f"# {name} noun semantic classes. Generated by build_data.py:\n"
                "#   WordNet lexname first pass + agentive/override layers.\n"
                "# `source` = wordnet | agentive | override | default (default = review me).\n")
        yaml.safe_dump({"nouns": noun_out}, f, sort_keys=True, default_flow_style=False)

    with open(os.path.join(OUT, f"verb_frames_{name}.yaml"), "w") as f:
        f.write(f"# {name} verb selectional-restriction archetypes. Generated by build_data.py.\n")
        yaml.safe_dump({"archetypes": ARCHETYPES, "verbs": verb_out},
                       f, sort_keys=True, default_flow_style=False)

    print(f"\n=== {name} ===")
    print(f"NOUNS: {len(nouns)}  by source: {src_count}")
    print(f"  by top class: {class_count}")
    print(f"  defaults needing review ({len(defaults)}): {defaults[:30]}"
          + (" ..." if len(defaults) > 30 else ""))
    print(f"VERBS: {len(verbs)}  by source: {vsrc_count}")
    print(f"  by archetype: {dict(sorted(arch_count.items(), key=lambda x:-x[1]))}")


def main():
    bip = yaml.safe_load(open(BIP39))
    cover = yaml.safe_load(open(COVER))
    # Agentive lift applies across both wordlists.
    global AGENTIVE
    AGENTIVE = {w for w in AGENTIVE_CANDIDATES if w in bip or w in cover}

    classify_wordlist("bip39", bip)
    classify_wordlist("cover", cover)
    print(f"\n  agentive words: {sorted(AGENTIVE)}")


if __name__ == "__main__":
    main()
