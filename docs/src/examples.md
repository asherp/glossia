# Examples

## Encoding a BIP39 Seed Phrase

Generate a 12-word seed phrase as natural prose:

```bash
$ glossia --random 12 --seed 0
```

```
Snake set robot to the mixed ship. Each program is night to each ten shield.
Bamboo own way to the yellow.
```

The embedded words are: `snake`, `robot`, `mixed`, `ship`, `program`, `night`, `ten`, `shield`, `bamboo`, `own`, `way`, `yellow`.

## Compact vs Natural Mode

**Natural mode** (default for body grammar) produces longer, more varied sentences:

```bash
$ glossia --random 12 --seed 0
```

**Compact mode** produces the shortest possible output:

```bash
$ glossia --random 12 --dialect body --length-mode compact --seed 0
```

```
Snake see robot. Ban is mixed. Ship program tap. Night is ten.
Shield get bamboo. Pie own way. Yellow is set.
```

## Subject Lines

Short phrases suitable for email subject lines:

```bash
$ glossia --random 12 --dialect subject --seed 0
```

```
A snake is out a robot is mixed the ship is pop the program is night the ten is
hot a shield is cut a bamboo is own a way is yellow
```

## Encoding an API Key

```bash
$ glossia --from-ascii "sk_live_51H3ll0W0r1d" --seed 0
```

```
Ban inflict foot to swallow for spray. Green may question. State get cinnamon.
Cricket see the gloom to army. The rid purpose already board the rid mosquito.
```

## Encoding an IP Address

```bash
$ glossia --from-ascii "127.0.0.1" --dialect subject --length-mode compact --seed 0
```

```
A couple is out a museum is pop each slide is set a gate is ago a toast is bad
the blade is hot the series is big
```

## Highlighting Payload Words

See which words carry the payload with bars:

```bash
$ glossia --random 6 --seed 0 --highlight-payload bars
```

Or with color (terminal only):

```bash
$ glossia --random 6 --seed 0 --highlight-payload green
```

## Mad-lib Mode

See the grammatical skeleton without payload words:

```bash
$ glossia --random 6 --seed 0 --madlib
```

Payload words are replaced with `[POS]` placeholders like `[N]`, `[V]`, `[Adj]`.

## Merkle Proofs

Wrap payload in a Merkle proof structure:

```bash
$ glossia --random 6 --merkle --seed 0
```

Extract the original payload from a merkleized sequence:

```bash
$ glossia --demerkle <merkleized words...>
```

Highlight both payload and Merkle words with different colors:

```bash
$ glossia --random 6 --merkle --highlight-payload green --highlight-merkle blue --seed 0
```

## Multiple Variations

Generate 3 variations and select the most compact:

```bash
$ glossia --random 12 --variations 3 --seed 0
```

This prints all variations plus statistics:

```
=== Statistics ===
Input:
  Total words: 12
  POS breakdown:
    Nouns: 10
    Verbs: 6
    Adjectives: 5
    Adverbs: 1
Output:
  Total words: 32
  Payload words: 12 (37.5%)
  Cover words: 20 (62.5%)
Compactness:
  Score: 0.580 (58 BIP39 chars / 100 total chars)
Variations:
  Tested: 3
  Min compactness: 0.523
  Max compactness: 0.580
  Improvement: 5.8% better than average
```

## Payload-Only Mode

Output just the payload words without grammar:

```bash
$ glossia --random 12 --dialect payload_only --seed 0
```

With a custom delimiter:

```bash
$ glossia --random 12 --dialect payload_only --delimiter ", " --seed 0
```

## Reading from Stdin

Encode text piped from another command:

```bash
echo "my-api-key-12345" | glossia --from-ascii -
```
