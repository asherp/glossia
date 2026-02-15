# Lambda Expression for Merkelization Process

## Process Overview

The merkelization process transforms a list of prime numbers (leaves) into a Merkle tree structure:

1. **Input**: List of leaf primes `L = [p₁, p₂, ..., pₙ]`
2. **Generate Merkle primes**: Generate `n-1` primes strictly greater than `max(L)`
3. **Build tree bottom-up**: Recursively pair nodes, creating parent nodes
4. **Assign primes top-down**: Root gets largest prime, descending by level (BFS)
5. **Pre-order traversal**: Output sequence as `[root, left_subtree, right_subtree]`

## Key Properties

- **Disjoint sets**: All Merkle (internal) primes > all leaf primes
- **Order preservation**: Input order maintained at leaf level
- **Deterministic parsing**: Merkle nodes identifiable by value (> max leaf)

## Lambda Expression

The merkelization process can be expressed as a higher-order function:

```
merkleize = λleaves: List[Prime]. λgen_merkle: (Prime → List[Prime]). λpair: (Node × Node → Node). λassign: (Tree × List[Prime] → Tree). λtraverse: (Tree → List[Prime]).
  let max_leaf = max(leaves) in
  let n = length(leaves) in
  let merkle_primes = gen_merkle(max_leaf, n-1) in
  let tree = build_tree(leaves, merkle_primes, pair) in
  let tree_assigned = assign(tree, reverse(merkle_primes)) in
  traverse(tree_assigned)
```

### Simplified Core Expression

The core recursive tree-building operation:

```
build_tree = λnodes: List[Node]. λmerkle_primes: List[Prime]. λpair: (Node × Node × Prime → Node).
  if length(nodes) == 1 then
    nodes[0]
  else
    let next_level = [] in
    let i = 0 in
    let prime_idx = 0 in
    while i < length(nodes):
      if i+1 >= length(nodes) then
        next_level.append(nodes[i])  // Carry odd node up
      else
        let parent = pair(nodes[i], nodes[i+1], merkle_primes[prime_idx]) in
        next_level.append(parent)
        prime_idx = prime_idx + 1
      i = i + 2
    build_tree(next_level, merkle_primes[prime_idx:], pair)
```

### Pre-order Traversal Expression

```
preorder = λnode: Node.
  if is_leaf(node) then
    [node.data]
  else
    [node.data] ++ preorder(node.left) ++ preorder(node.right)
```

### Complete Lambda Expression (Functional Style)

Using Church encoding for lists and recursion:

```
merkleize = λleaves: List[Prime].
  let max_leaf = fold(max, leaves) in
  let n = length(leaves) in
  let merkle_primes = generate_primes_after(max_leaf, n-1) in
  
  // Build tree bottom-up
  let build_level = λnodes: List[Node]. λprimes: List[Prime].
    if length(nodes) == 1 then
      nodes[0]
    else
      let pairs = zip(nodes[::2], nodes[1::2]) in
      let parents = map(λ(pair, prime). pair(pair[0], pair[1], prime), zip(pairs, primes)) in
      build_level(parents, primes[length(pairs):])
  in
  
  // Assign primes top-down (BFS)
  let assign_bfs = λtree: Tree. λprimes: List[Prime].
    let bfs_nodes = collect_bfs(tree) in  // Collect internal nodes in BFS order
    let assigned = map(λ(node, prime). node.data = prime, zip(bfs_nodes, primes)) in
    tree
  in
  
  // Pre-order traversal
  let preorder = λnode: Node.
    if is_leaf(node) then
      [node.data]
    else
      [node.data] ++ preorder(node.left) ++ preorder(node.right)
  in
  
  let tree = build_level(map(leaf, leaves), merkle_primes) in
  let tree_assigned = assign_bfs(tree, reverse(merkle_primes)) in
  preorder(tree_assigned)
```

### Type Signature

```
merkleize : List[Prime] → List[Prime]

where:
  - Input: List of n leaf primes
  - Output: Pre-order traversal of Merkle tree (2n-1 primes total)
  - Internal nodes: n-1 primes, all > max(input)
  - Leaves: n primes (original input, order preserved)
```

### Key Operations

1. **pair**: `Node × Node × Prime → Node`
   - Combines two nodes with a Merkle prime to create parent

2. **generate_primes_after**: `Prime × Nat → List[Prime]`
   - Generates n primes strictly greater than given prime

3. **build_level**: `List[Node] × List[Prime] → Tree`
   - Recursively builds tree levels bottom-up

4. **assign_bfs**: `Tree × List[Prime] → Tree`
   - Assigns primes to internal nodes in BFS order (root first, largest prime)

5. **preorder**: `Tree → List[Prime]`
   - Generates pre-order traversal sequence

## Example

For input `[2, 5, 11, 23]`:
- `max_leaf = 23`
- `n = 4`, so need `3` Merkle primes > 23
- Generate: `[29, 31, 37]` (example)
- Build tree bottom-up, then assign top-down (root gets 37)
- Pre-order: `[37, 29, 2, 5, 31, 11, 23]`

## Relationship to Glossia Grammar

This lambda expression could be integrated into Glossia's grammar system to:
- Generate POS sequences that represent Merkle tree structures
- Use Merkle tree structure as a grammatical pattern
- Embed prime sequences in natural language using the grammar system
