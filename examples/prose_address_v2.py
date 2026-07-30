#!/usr/bin/env python3
"""Prose address format v2 (glossia#76).

Script opcodes are rendered verbatim as Unicode symbols; Glossia encodes only the
entropy-carrying data (the hash160 or witness program). The format header rides in
the bit-packing slack that the codec currently zero-fills, so it costs no words:

    20-byte program : 160 data bits + 5 header bits = 165 = 15 words exactly
    32-byte program : 256 data bits + 8 header bits = 264 = 24 words exactly

Header (LSB-aligned in the tail): [log2_wordlist : 4][version : 1 or 4]
log2 is enough because payload wordlists are powers of two (bip39 = 2^11,
latin = 2^15), so 4 bits covers every list up to 2^15.

The cover realization is seeded from CRC-32 of the encoded bits, so the choice of
prose carries the checksum at zero length cost.

Opcode symbols are drawn only from non-alphanumeric Unicode: the decoder's token
trim (`trim_matches(|c| !c.is_alphanumeric())`) strips them, so they are invisible
to decoding even when adjacent to a payload word.
"""
import subprocess, sys, os, yaml, zlib

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
CLI = os.path.join(ROOT, 'target/release/glossia')
COUNTER_RANGE = 4
VERSION = 1

OPS = {'DUP': '⧉', 'HASH160': '⌗', 'EQUALVERIFY': '≟', 'CHECKSIG': '✓',
       'EQUAL': '⩵', 'WIT0': '▽', 'WIT1': '△'}

TEMPLATES = {                      # (leading symbols, trailing symbols)
    'P2PKH':  ([OPS['DUP'], OPS['HASH160']], [OPS['EQUALVERIFY'], OPS['CHECKSIG']]),
    'P2SH':   ([OPS['HASH160']],             [OPS['EQUAL']]),
    'P2WPKH': ([OPS['WIT0']],                []),
    'P2WSH':  ([OPS['WIT0']],                []),
    'P2TR':   ([OPS['WIT1']],                []),
}


def load_wordlist():
    class S(yaml.SafeLoader):
        pass
    S.add_constructor('tag:yaml.org,2002:bool', lambda l, n: l.construct_scalar(n))
    d = yaml.load(open(os.path.join(ROOT, 'languages/english/payload_bip39.yaml')), Loader=S)
    ws = d['words'] if isinstance(d, dict) and 'words' in d else list(d)
    return [str(w) for w in ws]


def mix64(x):
    U64 = (1 << 64) - 1
    z = (x + 0x9E3779B97F4A7C15) & U64
    z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) & U64
    z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & U64
    return (z ^ (z >> 31)) & U64


def pack(program, bits_per_word, log2_wordlist, version):
    """program bytes + header -> word indices, filling the slack exactly."""
    data_bits = len(program) * 8
    n_words = -(-data_bits // bits_per_word)
    slack = n_words * bits_per_word - data_bits
    assert slack >= 5, f'need >=5 slack bits, got {slack}'
    header = (log2_wordlist << (slack - 4)) | (version & ((1 << (slack - 4)) - 1))
    val = int.from_bytes(program, 'big') << slack | header
    return [(val >> (bits_per_word * (n_words - 1 - i))) & ((1 << bits_per_word) - 1)
            for i in range(n_words)], slack


def unpack(indices, bits_per_word, n_program_bytes):
    val = 0
    for i in indices:
        val = (val << bits_per_word) | i
    total = len(indices) * bits_per_word
    slack = total - n_program_bytes * 8
    header = val & ((1 << slack) - 1)
    program = (val >> slack).to_bytes(n_program_bytes, 'big')
    return program, header >> (slack - 4), header & ((1 << (slack - 4)) - 1)


def render(words, seed):
    out = subprocess.run([CLI, '--seed', str(seed), '--width', '400'] + words,
                         capture_output=True, text=True, timeout=300)
    if out.returncode != 0:
        raise RuntimeError(out.stderr[:300])
    return ' '.join(out.stdout.split())


def payload_tokens(text, wordset):
    toks = []
    for w in text.split():
        bare = w.strip(''.join(c for c in w if not c.isalnum())).lower() if w else ''
        bare = ''.join(ch for ch in bare)
        # mirror Rust: trim non-alphanumeric from both ends, lowercase
        s, e = 0, len(w)
        while s < e and not w[s].isalnum():
            s += 1
        while e > s and not w[e - 1].isalnum():
            e -= 1
        bare = w[s:e].lower()
        if bare and bare in wordset:
            toks.append(bare)
    return toks


def main():
    wordlist = load_wordlist()
    wordset = set(wordlist)
    bits_per_word = (len(wordlist) - 1).bit_length()   # 2048 -> 11
    log2_wl = bits_per_word
    idx = {w: i for i, w in enumerate(wordlist)}
    print(f'payload wordlist: english/bip39, {len(wordlist)} words, '
          f'{bits_per_word} bits/word (log2 = {log2_wl})\n')

    for line in sys.stdin:
        parts = line.rstrip('\n').split('\t')
        if len(parts) < 4:
            continue
        label, addr, stype, program_hex = parts[0], parts[1], parts[2], parts[3]
        program = bytes.fromhex(program_hex)
        indices, slack = pack(program, bits_per_word, log2_wl, VERSION)
        words = [wordlist[i] for i in indices]
        checksum = zlib.crc32(bytes(program) + bytes([log2_wl, VERSION]))

        best = None
        for c in range(COUNTER_RANGE):
            seed = mix64((checksum << 32 | c) & ((1 << 64) - 1))
            prose = render(words, seed)
            density = len(words) / max(1, len(prose.split()))
            if best is None or density > best[0]:
                best = (density, prose, c, seed)
        _, prose, counter, seed = best

        lead, trail = TEMPLATES[stype]
        artifact = ' '.join(lead + [prose] + trail)

        print('─' * 78)
        print(f'{label}\n{addr}')
        print(f'  {stype} · {len(program)}-byte program · {len(words)} payload words · '
              f'{slack}-bit header (free) · crc32 {checksum:08x} · counter {counter}')
        print(f'\n{artifact}\n')

        # decode
        toks = payload_tokens(artifact, wordset)
        ok_count = len(toks) == len(words)
        prog2, log2_2, ver2 = unpack([idx[t] for t in toks], bits_per_word, len(program)) \
            if ok_count else (b'', -1, -1)
        # verify by re-render
        verdict = 'decoded, unverified'
        if ok_count and prog2 == program:
            for c in range(COUNTER_RANGE):
                s2 = mix64((zlib.crc32(bytes(prog2) + bytes([log2_2, ver2])) << 32 | c)
                           & ((1 << 64) - 1))
                if render([wordlist[i] for i in [idx[t] for t in toks]], s2).split() == prose.split():
                    verdict = f'VERIFIED (counter {c})'
                    break
        print(f'  round-trip: program {"exact" if prog2 == program else "MISMATCH"}, '
              f'header log2={log2_2} version={ver2}   {verdict}')


if __name__ == '__main__':
    main()
