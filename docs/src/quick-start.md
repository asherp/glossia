# Quick Start

## Embed BIP39 Words in Prose

Provide words directly as arguments:

```bash
$ glossia abandon ability able about above absent
```

```
The abandon may see the ability to each able ears about above. Guy may absent.
```

The BIP39 words are embedded in-order within grammatically correct sentences. To decode, simply filter the output against the BIP39 wordlist.

## Generate Random Words

```bash
$ glossia --random 12 --seed 0
```

```
Snake set robot to the mixed ship. Each program is night to each ten shield.
Bamboo own way to the yellow.
```

## Encode ASCII Text

Encode arbitrary text (API keys, secrets, IP addresses) into natural language:

```bash
$ glossia --from-ascii "sk_live_51H3ll0W0r1d" --seed 0
```

```
Ban inflict foot to swallow for spray. Green may question. State get cinnamon.
Cricket see the gloom to army. The rid purpose already board the rid mosquito.
```

## Decode Back to Text

```bash
$ glossia --decode ban inflict foot swallow spray green question state cinnamon cricket gloom army rid purpose already board rid mosquito
```

## Highlight Payload Words

Use `--highlight-payload` to see which words carry the payload:

```bash
$ glossia --random 6 --seed 0 --highlight-payload bars
```

This wraps payload words in `|delimiters|` so you can visually distinguish them from cover words.

## Next Steps

- [Usage](./usage.md) for a full CLI reference
- [How It Works](./how-it-works.md) for the encoding/decoding theory
- [Grammar](./grammar.md) for the grammar system
- [Examples](./examples.md) for more use cases
