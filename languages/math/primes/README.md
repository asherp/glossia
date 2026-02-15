# Merkle Tree Grammar for Glossia

This directory contains grammar rules for generating natural language text that follows Merkle tree structures, integrated with Glossia's lambda calculus-based grammar system.

## Files

- **`grammar.yaml`**: Glossia grammar file defining Merkle tree structures using POS sequences
- **`merkle_lambda.md`**: Full documentation of the lambda expressions for the merkelization process
- **`merkle_lambda_expression.txt`**: Concise lambda expressions for reference
- **`merkelize_primes.py`**: Python implementation of the merkelization algorithm
- **`wordlist.txt`**: List of primes used as the wordlist

## Grammar Structure

The grammar maps Merkle tree concepts to POS (Part of Speech) tags:

- **Root (Merkle root)** → `N` (noun, head of structure)
- **Leaf (leaf node)** → `Det` (determiner, atomic element)
- **Internal (internal node)** → `V` (verb, combines elements)
- **Sequence** → Pre-order traversal pattern

## Lambda Expressions

The full lambda expressions are documented in `merkle_lambda.md`. The core merkelization function:

```
merkleize = λleaves: List[Prime]. 
  let max_leaf = fold(max, leaves) in
  let n = length(leaves) in
  let merkle_primes = generate_primes_after(max_leaf, n-1) in
  let tree = build_tree(map(leaf, leaves), merkle_primes) in
  let tree_assigned = assign_bfs(tree, reverse(merkle_primes)) in
  preorder(tree_assigned)
```

## Usage

To use this grammar with Glossia:

1. Ensure the grammar.yaml file is in the correct location: `languages/math/primes/grammar.yaml`
2. Use the language identifier `math/primes` when running Glossia
3. The grammar will generate POS sequences following Merkle tree patterns
4. Words from the wordlist will be assigned to fill the POS slots

## Example Patterns

The grammar generates sequences like:

- **2-leaf tree**: `N Det Det Dot` (Root, Leaf, Leaf)
- **3-leaf tree**: `N V Det Det Det Dot` (Root, Internal(Leaf, Leaf), Leaf)
- **4-leaf balanced**: `N V Det Det V Det Det Dot` (Root, Internal(Leaf, Leaf), Internal(Leaf, Leaf))

These POS sequences can then be filled with words from the wordlist to generate natural language text that structurally follows Merkle tree patterns.

## Integration with Merkelization

The grammar is designed to work alongside the `merkelize_primes.py` script:

1. Use `merkelize_primes.py` to create Merkle tree structures from prime sequences
2. Use the grammar to generate POS sequences that match Merkle tree patterns
3. Fill POS slots with words from the wordlist
4. The resulting text structurally represents the Merkle tree while being readable as natural language

## Properties

- **Disjoint sets**: All Merkle (internal) primes > all leaf primes
- **Order preservation**: Input order maintained at leaf level
- **Deterministic parsing**: Merkle nodes identifiable by value
- **Pre-order traversal**: Output follows root → left → right pattern
