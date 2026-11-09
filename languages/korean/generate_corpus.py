#!/usr/bin/env python3
"""Generate languages/korean/payload.yaml, Korean's default payload list:
16,384 words (2^14) drawn from a frequency corpus, the way Latin's list was drawn
from Wiktionary.

Source: Shiberu Korean Word Frequency List 2026 by Shiberu
(https://shiberu.app/korean-frequency-list), licensed under CC BY 4.0.
Release 2026.10.28, ko-frequency-2026.tsv, sha256
6ed27cd6acbd67275517a282220ae47be24efd7d79b49acad079d70c13ccb124.

Selection, in Shiberu rank order (most frequent first):

  1. All-Hangul lemmas only.
  2. Kiwi (kiwipiepy 0.24.1) must analyse the lemma as ONE morpheme its own
     lexicon holds, tagged common noun (NNG), adverb (MAG) or prenoun (MM).
     That drops proper nouns, out-of-vocabulary guesses (히가시, 앵그리버드)
     and spaceless phrases (사회변화).
  3. Two to five syllables.
  4. Not a cover word, and not on BLOCKLIST below.
  5. Particle safety. A word's forms are the word itself and the word plus
     each of the 13 particles in KOREAN_PARTICLES (every allomorph, not just
     the one the agreement pass would pick). A word is kept only if none of
     its forms equals a form of a word already kept, or a cover word, or a
     cover noun/pronoun plus a particle. That is exactly the pair of
     properties `codec::payload_form` relies on, pinned in tests/korean.rs.
  6. The first 16,384 survivors form the list.

The kept words are written in Hangul (code point) order, which is 가나다
order and the order the BIP39 Korean list uses. **Index i is the i-th key of
payload.yaml. Once shipped, the file is append-only and frozen.**
payload_provenance.tsv records each word's Shiberu rank and rate.

Usage:
    pip install kiwipiepy==0.24.1
    python3 languages/korean/generate_corpus.py ko-frequency-2026.tsv
"""
import argparse
import hashlib
import os
import re
import sys
import unicodedata

from kiwipiepy import Kiwi

HERE = os.path.dirname(os.path.abspath(__file__))
SIZE = 1 << 14
EXPECTED_SHA256 = "6ed27cd6acbd67275517a282220ae47be24efd7d79b49acad079d70c13ccb124"

# Same list, same order as KOREAN_PARTICLES in src/codec.rs.
PARTICLES = "이 가 은 는 을 를 에 에서 와 과 의 로 으로".split()
HANGUL = re.compile(r"^[가-힣]+$")
TAGS = {"NNG": "noun", "MAG": "adverb", "MM": "prenoun"}

# Words a frequency list ranks high that a payload list should not carry:
# vulgar or insulting words, slurs, sexual and violent-crime terms, and
# politically loaded labels. This is a first pass by a non-native reviewer,
# not a substitute for the native-speaker review BIP39 Korean had.
BLOCKLIST = set("""
몰카 꼴값 도둑놈 개새끼 새끼 병신 지랄 염병 씨발 좆 개소리 개판 개념녀 김치녀 된장녀
한남충 맘충 틀딱 급식충 일베 홍어 쪽바리 짱깨 깜둥이 흑형 장애자 정신병자 미친놈 미친년
또라이 찐따 호구 등신 머저리 쌍놈 잡놈 잡년 화냥년 창녀 매춘부 매춘 성매매 원조교제
강간 성폭행 성추행 성노동 페티시 포르노 야동 섹스 자위 음란물 변태 몸캠
자살 자해 살인마 토막 학살 고문 물고문
똥구멍 불알 정액 젖꼭지 젖가슴 성기 성욕 음란 콘돔 누드 알몸 나체 섹시 발정 암캐 걸레
꼴통 미친개 딸년 내연녀 유부녀 이혼녀 간통 간통죄 불륜 강간범 강간죄 폭행 살인범 살인자
살인죄 마약 필로폰 대마초 테러리스트 테러범 매국노 왜구 동성애자 양성애자
귀머거리 난쟁이 돌대가리 멍청이 못난이 바보 백치 벙어리 불구 사생아 양아치 얼간이 쪼다
천치 저능아 장님 잡종 꼰대 오르가즘 음경 음부 항문 후장 발기 포주 룸살롱
게이 트랜스젠더 위안부 친일파 극우 좌파 우파 수구
적폐 적폐세력 빨갱이 수꼴 좌빨 종북 토착왜구 대깨문 문빠 박사모 태극기부대
""".split())
# Substrings that mark a whole family of insults (놈/년 compounds etc.).
BLOCK_SUBSTRINGS = ("새끼", "씨발", "좆", "병신", "년놈")
BLOCK_SUFFIXES = ("놈",)


def nfc(s):
    return unicodedata.normalize("NFC", s)


def forms(word):
    return {word} | {word + p for p in PARTICLES}


def single_morpheme(kiwi, lemma):
    toks = kiwi.analyze(lemma)[0][0]
    if len(toks) != 1 or toks[0].id == 2 or toks[0].form != lemma:
        return None
    return TAGS.get(toks[0].tag)


def load_cover(path):
    """Cover keys, and the ones that take particles (N or Pron)."""
    cover, takes = set(), set()
    cur = None
    with open(path, encoding="utf-8") as f:
        for line in f:
            m = re.match(r"^([^\s#][^:]*):\s*$", line)
            if m:
                cur = nfc(m.group(1).strip().strip("'\""))
                cover.add(cur)
            elif cur and re.match(r"^\s+(N|Pron):", line):
                takes.add(cur)
    return cover, takes


def blocked(word):
    return (word in BLOCKLIST
            or any(s in word for s in BLOCK_SUBSTRINGS)
            or (len(word) >= 2 and word.endswith(BLOCK_SUFFIXES)))


def pos_entry(word, cat):
    if cat == "adverb":
        return [("Adv", 1.0)]
    if cat == "prenoun":
        return [("Adj", 1.0)]
    if word.endswith("적") and len(word) >= 3:
        # Sino-Korean -적 forms stand before a noun (구체적 계획), as in payload.yaml.
        return [("Adj", 1.0), ("N", 0.3)]
    return [("N", 1.0)]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("tsv", help="ko-frequency-2026.tsv")
    ap.add_argument("--cover", default=os.path.join(HERE, "cover.yaml"))
    args = ap.parse_args()

    raw = open(args.tsv, "rb").read()
    digest = hashlib.sha256(raw).hexdigest()
    if digest != EXPECTED_SHA256:
        sys.exit(f"{args.tsv}: sha256 {digest} is not the pinned 2026.10.28 release")

    cover, takes = load_cover(args.cover)
    reserved = set(cover)
    for c in takes:
        reserved |= forms(c)

    kiwi = Kiwi()
    taken = set()
    kept = []   # (word, cat, rank, per_million)
    for line in raw.decode("utf-8").splitlines()[1:]:
        rank, lemma, pm = line.split("\t")
        w = nfc(lemma)
        if not HANGUL.match(w) or not 2 <= len(w) <= 5:
            continue
        if w in cover or blocked(w):
            continue
        cat = single_morpheme(kiwi, w)
        if cat is None:
            continue
        f = forms(w)
        if f & reserved or f & taken:
            continue
        taken |= f
        kept.append((w, cat, int(rank), pm))
        if len(kept) == SIZE:
            break
    if len(kept) < SIZE:
        sys.exit(f"only {len(kept)} words survived; need {SIZE}")

    kept.sort(key=lambda e: e[0])
    out = os.path.join(HERE, "payload.yaml")
    with open(out, "w", encoding="utf-8") as f:
        f.write(
            "# Korean payload wordlist for Glossia (2^14 = 16,384 words, 14 bits/word)\n"
            "# The official BIP39 Korean list is payload_bip39.yaml (--wordlist bip39).\n"
            "# Source: Shiberu Korean Word Frequency List 2026 by Shiberu\n"
            "#   (https://shiberu.app/korean-frequency-list), licensed under CC BY 4.0.\n"
            "#   Release 2026.10.28. Selection: languages/korean/generate_corpus.py.\n"
            "#\n"
            "# Index i is the i-th key below (Hangul order). APPEND-ONLY and frozen.\n"
            "# Shiberu rank and rate per word: payload_provenance.tsv.\n"
            "#\n")
        for w, cat, _, _ in kept:
            f.write(f"{w}:\n")
            for tag, weight in pos_entry(w, cat):
                f.write(f"  {tag}: {weight}\n")
    with open(os.path.join(HERE, "payload_provenance.tsv"), "w", encoding="utf-8") as f:
        f.write("index\tword\tpos\tshiberu_rank\tper_million\n")
        for i, (w, cat, rank, pm) in enumerate(kept):
            f.write(f"{i}\t{w}\t{cat}\t{rank}\t{pm}\n")
    last = max(kept, key=lambda e: e[2])
    print(f"wrote {len(kept)} words to {out}; deepest Shiberu rank {last[2]} "
          f"({last[0]}, {last[3]}/M)")


if __name__ == "__main__":
    main()
