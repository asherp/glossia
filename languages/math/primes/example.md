# Merkle Grammar Example

This document demonstrates how the Merkle tree grammar works with a concrete example.

## Step 1: Input Primes (Leaves)

Let's start with 4 leaf primes:
```
[2, 5, 11, 23]
```

## Step 2: Merkelization Process

Using `merkelize_primes.py`, we merkleize these primes:

```bash
python merkelize_primes.py "2,5,11,23"
```

### Process:
1. **Find max leaf**: `max(leaves) = 23`
2. **Generate Merkle primes**: Need `n-1 = 3` primes > 23
   - Generate: `[29, 31, 37]` (all > 23)
3. **Build tree bottom-up**:
   ```
   Level 0 (leaves):     [2]  [5]  [11]  [23]
   Level 1 (pairs):      [29(2,5)]  [31(11,23)]
   Level 2 (root):       [37(29,31)]
   ```
4. **Assign primes top-down** (BFS):
   - Root gets largest: `37`
   - Level 1 gets: `31, 29` (left to right)
5. **Pre-order traversal**: `[37, 29, 2, 5, 31, 11, 23]`

### Output Sequence:
```
37, 29, 2, 5, 31, 11, 23
```

Where:
- **Merkle nodes** (internal): `37, 29, 31` (all > 23)
- **Leaves**: `2, 5, 11, 23` (original input)

## Step 3: Grammar Pattern Matching

The grammar generates POS sequences that match Merkle tree structures. For a 4-leaf balanced tree, the pattern is:

```
N V Det Det V Det Det Dot
```

This represents:
- `N` = Root (37)
- `V Det Det` = Left internal node (29) with leaves (2, 5)
- `V Det Det` = Right internal node (31) with leaves (11, 23)
- `Dot` = End marker

## Step 4: Word Assignment

Using Glossia's wordlist system, POS slots are filled:

### Example Wordlist Mapping:
- **N (nouns)**: "tree", "root", "structure", "node"
- **V (verbs)**: "combines", "merges", "pairs", "joins"
- **Det (determiners)**: "the", "a", "each", "some"
- **Dot**: "."

### Possible Output:
```
"tree combines the a pairs each some."
```

Or with better word selection:
```
"root pairs the two combines each pair."
```

## Step 5: Complete Example Workflow

### Input:
```python
leaves = [2, 5, 11, 23]
```

### Merkelization:
```python
sequence = [37, 29, 2, 5, 31, 11, 23]
tree_structure = {
    root: 37,
    left: {
        node: 29,
        left: 2,
        right: 5
    },
    right: {
        node: 31,
        left: 11,
        right: 23
    }
}
```

### Grammar Pattern:
```
N V Det Det V Det Det Dot
```

### POS Sequence with Primes:
```
N(37) V(29) Det(2) Det(5) V(31) Det(11) Det(23) Dot
```

### Natural Language Output (example):
```
"root pairs the two combines each pair."
```

## Visual Representation

```
Merkle Tree Structure:
        37 (N - root)
       /  \
     29    31 (V - internal nodes)
    / \   / \
   2  5  11 23 (Det - leaves)
```

```
Grammar Pattern:
N    V    Det Det  V    Det Det  Dot
root pairs the two combines each pair .
```

## Verification

To verify the merkleized sequence:

```bash
python merkelize_primes.py --verify "37,29,2,5,31,11,23"
```

Expected output:
```
valid
```

To extract leaves:

```bash
python merkelize_primes.py -p "37,29,2,5,31,11,23"
```

Expected output:
```
2,5,11,23
```

## Integration with Glossia

When using Glossia with the `math/primes` grammar:

1. **Generate POS sequence** using grammar rules:
   ```rust
   // Grammar generates: N V Det Det V Det Det Dot
   ```

2. **Fill slots** with words from wordlist:
   ```rust
   // N → "root" (from wordlist)
   // V → "pairs" (from wordlist)
   // Det → "the", "two" (from wordlist)
   // etc.
   ```

3. **Output natural language**:
   ```
   "root pairs the two combines each pair."
   ```

## Different Tree Sizes

### 2-Leaf Tree
- **Input**: `[2, 5]`
- **Merkelized**: `[7, 2, 5]` (where 7 > 5)
- **Pattern**: `N Det Det Dot`
- **Example**: `"root the two."`

### 3-Leaf Tree
- **Input**: `[2, 5, 11]`
- **Merkelized**: `[17, 13, 2, 5, 11]` (where 13, 17 > 11)
- **Pattern**: `N V Det Det Det Dot`
- **Example**: `"root pairs the two three."`

### 4-Leaf Balanced Tree (shown above)
- **Pattern**: `N V Det Det V Det Det Dot`
- **Example**: `"root pairs the two combines each pair."`

## Notes

- The grammar preserves the **structural relationship** of the Merkle tree
- Words are selected from the wordlist based on POS requirements
- The **order** of primes in the sequence matches the pre-order traversal
- **Merkle nodes** (internal) are always larger than **leaf nodes** (disjoint sets property)
