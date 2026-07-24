#!/usr/bin/env python3
"""Bitcoin address -> prose-address payload bytes.

Payload layout (proposed v1):
    [0]     version      profile id: grammar + semantics + RNG + cover mode
    [1]     script_type  1=P2PKH 2=P2SH 3=P2WPKH 4=P2WSH 5=P2TR
    [2..4]  wordlist_len u16 big-endian, payload wordlist size (allows extension)
    [4..]   program      the hash160 / witness program (opcodes implied by type)

Storing type+program rather than the full scriptPubKey saves the opcode bytes:
a P2PKH scriptPubKey is 25 bytes but its hash160 is only 20.
"""

B58 = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz'
BECH32 = 'qpzry9x8gf2tvdw0s3jn54khce6mua7l'


def b58check_decode(s):
    n = 0
    for c in s:
        n = n * 58 + B58.index(c)
    raw = n.to_bytes(25, 'big')
    import hashlib
    body, chk = raw[:21], raw[21:]
    d = hashlib.sha256(hashlib.sha256(body).digest()).digest()[:4]
    assert d == chk, f'base58 checksum failed for {s}'
    return body[0], body[1:]


def bech32_polymod(values):
    gen = [0x3b6a57b2, 0x26508e6d, 0x1ea119fa, 0x3d4233dd, 0x2a1462b3]
    chk = 1
    for v in values:
        b = chk >> 25
        chk = (chk & 0x1ffffff) << 5 ^ v
        for i in range(5):
            chk ^= gen[i] if ((b >> i) & 1) else 0
    return chk


def bech32_hrp_expand(hrp):
    return [ord(x) >> 5 for x in hrp] + [0] + [ord(x) & 31 for x in hrp]


def convertbits(data, frombits, tobits, pad=True):
    acc, bits, ret = 0, 0, []
    maxv = (1 << tobits) - 1
    for value in data:
        acc = (acc << frombits) | value
        bits += frombits
        while bits >= tobits:
            bits -= tobits
            ret.append((acc >> bits) & maxv)
    if pad and bits:
        ret.append((acc << (tobits - bits)) & maxv)
    return ret


def bech32_decode(addr):
    hrp, data_part = addr.rsplit('1', 1)
    data = [BECH32.index(c) for c in data_part]
    const = bech32_polymod(bech32_hrp_expand(hrp) + data)
    witver = data[0]
    expected = 1 if witver == 0 else 0x2bc830a3   # bech32 vs bech32m
    assert const == expected, f'bech32 checksum failed for {addr} (got {const:#x})'
    program = bytes(convertbits(data[1:-6], 5, 8, False))
    return witver, program


ADDRESSES = [
    ('P2PKH  (genesis coinbase)', '1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa'),
    ('P2SH', '3J98t1WpEZ73CNmQviecrnyiWrnqRhWNLy'),
    ('P2WPKH (BIP173 vector)', 'bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4'),
    ('P2WSH  (BIP173 vector)',
     'bc1qrp33g0q5c5txsp9arysrx4k6zdkfs4nce4xj0gdcccefvpysxf3qccfmv3'),
    ('P2TR   (BIP86 vector)',
     'bc1pmfr3p9j00pfxjh0zmgp99y8zftmd3s5pmedqhyptwy6lm87hf5sspknck9'),
]

VERSION = 1
WORDLIST_LEN = 2048   # english bip39

for label, addr in ADDRESSES:
    if addr[0] in '13':
        ver, program = b58check_decode(addr)
        stype = 1 if ver == 0x00 else 2
    else:
        witver, program = bech32_decode(addr)
        stype = {0: (3 if len(program) == 20 else 4), 1: 5}[witver]
    header = bytes([VERSION, stype]) + WORDLIST_LEN.to_bytes(2, 'big')
    payload = header + program
    print(f'{label}\t{addr}\t{payload.hex()}\t{len(payload)}')
