#!/usr/bin/env python3
"""
Merkleize a sequence of primes using the disjoint sets convention.

The disjoint sets convention:
- Merkle (internal) node primes are LARGER than all leaf primes
- Input order is preserved in the output
- Parsing is deterministic because Merkle nodes are identifiable by value (> max leaf)

Usage:
  python merkelize_primes.py "2,5,11,23"            # Merkleize from argument
  echo "2,5,11,23" | python merkelize_primes.py     # Merkleize from stdin
  python merkelize_primes.py -N 4                   # Generate 4 random primes
  python merkelize_primes.py "..." | python merkelize_primes.py --verify  # Round-trip
  python merkelize_primes.py -p "37,29,2,5,31,11,23"  # Parse → extract leaves
"""

import sys
import os
import math
import random
import argparse

# Import functions from get_integers.py (in integers directory)
sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..', 'integers'))
from get_integers import (
    is_prime,
    generate_primes_up_to,
    generate_primes_up_to_inclusive,
    calculate_internal_nodes_needed,
    build_ordered_list_with_merkle_nodes
)


def generate_n_primes_after(start, n):
    """Generate n primes strictly greater than start.
    
    Args:
        start: Generate primes > start
        n: Number of primes to generate
    
    Returns:
        List of n primes in ascending order
    """
    primes = []
    candidate = start + 1
    while len(primes) < n:
        if is_prime(candidate):
            primes.append(candidate)
        candidate += 1
    return primes


def get_node_value(node):
    """Get the string representation of a node's value."""
    if node is None:
        return ""
    if node.get('type') == 'leaf':
        leaf_data = node['data']
        if isinstance(leaf_data, list) and len(leaf_data) == 1:
            return str(leaf_data[0])
        elif isinstance(leaf_data, list):
            return str(leaf_data)
        else:
            return str(leaf_data)
    elif node.get('type') == 'internal':
        return str(node['data'])
    return ""


def get_max_depth(node):
    """Calculate the maximum depth of the tree (depth of deepest leaf)."""
    if node is None:
        return 0
    if node.get('type') == 'leaf':
        return 1
    
    left_depth = get_max_depth(node.get('left')) if node.get('left') else 0
    right_depth = get_max_depth(node.get('right')) if node.get('right') else 0
    return 1 + max(left_depth, right_depth)


def collect_leaves_with_parents(node, parent_map=None, parent_data=None):
    """Recursively collect all leaves and map them to their parents.
    Uses parent's data (prime number) as key since nodes are not hashable."""
    if parent_map is None:
        parent_map = {}
    
    if node is None:
        return parent_map
    
    if node.get('type') == 'leaf':
        if parent_data is not None:
            if parent_data not in parent_map:
                parent_map[parent_data] = []
            parent_map[parent_data].append(node)
        return parent_map
    
    # Internal node - get its data (prime number) to use as key
    node_data = node.get('data')
    
    # Recurse to children
    if node.get('left'):
        collect_leaves_with_parents(node.get('left'), parent_map, node_data)
    if node.get('right'):
        collect_leaves_with_parents(node.get('right'), parent_map, node_data)
    
    return parent_map


def get_tree_levels(node):
    """Convert tree to levels for binomial-style visualization.
    Ensures all leaves are at the bottom level."""
    if node is None:
        return [], {}
    
    max_depth = get_max_depth(node)
    levels = []
    queue = [node]
    current_depth = 0
    all_leaves = []  # Collect all leaves to place at bottom
    parent_to_leaves = collect_leaves_with_parents(node)  # Map parents to their leaves
    
    # First pass: collect all leaves
    def collect_all_leaves(n):
        if n is None:
            return
        if n.get('type') == 'leaf':
            all_leaves.append(n)
        else:
            if n.get('left'):
                collect_all_leaves(n.get('left'))
            if n.get('right'):
                collect_all_leaves(n.get('right'))
    
    collect_all_leaves(node)
    
    # Second pass: build levels, replacing leaves with None
    queue = [node]
    current_depth = 0
    
    while queue and current_depth < max_depth:
        level_size = len(queue)
        level = []
        next_queue = []
        
        for _ in range(level_size):
            current = queue.pop(0)
            if current is None:
                level.append(None)
                if current_depth < max_depth - 1:
                    next_queue.append(None)
                    next_queue.append(None)
            else:
                if current.get('type') == 'leaf':
                    # Replace leaf with None at this level (will appear at bottom)
                    level.append(None)
                    if current_depth < max_depth - 1:
                        next_queue.append(None)
                        next_queue.append(None)
                else:
                    # Internal node
                    level.append(current)
                    if current_depth < max_depth - 1:
                        next_queue.append(current.get('left'))
                        next_queue.append(current.get('right'))
                    else:
                        next_queue.append(None)
                        next_queue.append(None)
        
        # Add level if it has at least one non-None node
        if any(n is not None for n in level):
            levels.append(level)
        
        current_depth += 1
        queue = next_queue
    
    # Add bottom level with all leaves
    if all_leaves:
        # Create bottom level with leaves evenly spaced
        bottom_level = [None] * (2 ** (max_depth - 1))  # Max possible nodes at bottom
        # Distribute leaves evenly
        for i, leaf in enumerate(all_leaves):
            if i < len(bottom_level):
                bottom_level[i] = leaf
        levels.append(bottom_level)
    
    return levels, parent_to_leaves


def draw_tree_visualization(node, prefix="", is_last=True, is_root=True):
    """Draw a tree visualization in file tree style (root on left, branches to right)."""
    if node is None:
        return
    
    MERKLE_COLOR = '\033[92m'  # Green
    RESET_COLOR = '\033[0m'     # Reset
    
    # Get node value
    value = get_node_value(node)
    is_colored = (node.get('type') == 'internal')
    
    # Print current node
    if is_root:
        # Root node - no prefix
        if is_colored:
            print(MERKLE_COLOR + value + RESET_COLOR)
        else:
            print(value)
    else:
        # Child node - use tree characters
        connector = "└── " if is_last else "├── "
        if is_colored:
            print(prefix + connector + MERKLE_COLOR + value + RESET_COLOR)
        else:
            print(prefix + connector + value)
    
    # Recursively draw children
    left = node.get('left')
    right = node.get('right')
    
    # Determine which children exist
    children = []
    if left:
        children.append(('left', left))
    if right:
        children.append(('right', right))
    
    # Draw each child
    for i, (child_name, child_node) in enumerate(children):
        is_last_child = (i == len(children) - 1)
        
        # Update prefix for child
        if is_root:
            child_prefix = ""
        else:
            if is_last:
                child_prefix = prefix + "    "  # 4 spaces for last child
            else:
                child_prefix = prefix + "│   "  # Vertical line + 3 spaces
        
        # Recursively draw child
        draw_tree_visualization(child_node, child_prefix, is_last_child, is_root=False)


def get_node_data(node):
    """Extract the data from a node (leaf or internal)."""
    if node.get('type') == 'leaf':
        return node['data']
    elif node.get('type') == 'internal':
        return node['data']
    return None


def load_wordlist(wordlist_path):
    """Load primes from wordlist.txt file.
    
    Args:
        wordlist_path: Path to wordlist.txt file
    
    Returns:
        List of primes from the wordlist
    """
    primes = []
    try:
        with open(wordlist_path, 'r') as f:
            for line in f:
                line = line.strip()
                if line:  # Skip empty lines
                    try:
                        prime = int(line)
                        primes.append(prime)
                    except ValueError:
                        continue  # Skip non-numeric lines
    except FileNotFoundError:
        print(f"Warning: Wordlist file not found at {wordlist_path}", file=sys.stderr)
        return []
    return primes


def find_leaf_nodes(node, target_primes, found_nodes=None):
    """Find leaf nodes in the tree that match target primes.
    
    Args:
        node: Root node of the tree
        target_primes: Set of primes to find
        found_nodes: Dictionary mapping prime -> node (modified in place)
    
    Returns:
        Dictionary mapping prime -> node
    """
    if found_nodes is None:
        found_nodes = {}
    
    if node is None:
        return found_nodes
    
    if node.get('type') == 'leaf':
        leaf_data = node.get('data')
        if isinstance(leaf_data, list):
            prime = leaf_data[0] if len(leaf_data) == 1 else None
        else:
            prime = leaf_data
        
        if prime in target_primes:
            found_nodes[prime] = node
    else:
        # Internal node - recurse to children
        if node.get('left'):
            find_leaf_nodes(node.get('left'), target_primes, found_nodes)
        if node.get('right'):
            find_leaf_nodes(node.get('right'), target_primes, found_nodes)
    
    return found_nodes


def get_merkle_proof_path(node, target_node, path=None):
    """Get the Merkle proof path from root to a target node.
    
    Args:
        node: Current node in traversal
        target_node: Target node to find path to
        path: List to accumulate path (modified in place)
    
    Returns:
        List of nodes from root to target (including target), or None if not found
    """
    if path is None:
        path = []
    
    if node is None:
        return None
    
    # Add current node to path
    path.append(node)
    
    # If we found the target, return the path
    if node == target_node:
        return path
    
    # Try left subtree
    if node.get('left'):
        result = get_merkle_proof_path(node.get('left'), target_node, path)
        if result is not None:
            return result
        # Backtrack - remove nodes added in failed search
        while len(path) > 1 and path[-1] != node:
            path.pop()
    
    # Try right subtree
    if node.get('right'):
        result = get_merkle_proof_path(node.get('right'), target_node, path)
        if result is not None:
            return result
        # Backtrack - remove nodes added in failed search
        while len(path) > 1 and path[-1] != node:
            path.pop()
    
    # Not found in this subtree - backtrack
    if path and path[-1] == node:
        path.pop()
    
    return None


def generate_merkle_proof_for_leaf(root, leaf_prime):
    """Generate a Merkle proof for a single leaf in the tree.
    
    Args:
        root: Root node of the Merkle tree
        leaf_prime: The prime number of the leaf to generate proof for
    
    Returns:
        Dictionary with:
            - leaf: The leaf prime
            - proof_path: List of primes from root to leaf
            - sibling_path: List of sibling primes needed for verification
    """
    # Find the leaf node
    target_primes = {leaf_prime}
    found_nodes = find_leaf_nodes(root, target_primes)
    
    if leaf_prime not in found_nodes:
        return None
    
    leaf_node = found_nodes[leaf_prime]
    
    # Get the path from root to leaf
    path = get_merkle_proof_path(root, leaf_node)
    
    if not path:
        return None
    
    result = {
        'leaf': leaf_prime,
        'proof_path': [],
        'sibling_path': []
    }
    
    # Extract primes from the path
    proof_path_primes = []
    for node in path:
        node_data = get_node_data(node)
        if isinstance(node_data, list):
            prime = node_data[0] if len(node_data) == 1 else node_data
        else:
            prime = node_data
        proof_path_primes.append(prime)
    result['proof_path'] = proof_path_primes
    
    # Build sibling path (siblings needed for verification)
    for i in range(len(path) - 1):
        current_node = path[i]
        next_node = path[i + 1]
        
        # Determine if next_node is left or right child
        if current_node.get('left') == next_node:
            sibling = current_node.get('right')
        elif current_node.get('right') == next_node:
            sibling = current_node.get('left')
        else:
            sibling = None
        
        if sibling:
            sibling_data = get_node_data(sibling)
            if isinstance(sibling_data, list):
                sibling_prime = sibling_data[0] if len(sibling_data) == 1 else sibling_data
            else:
                sibling_prime = sibling_data
            result['sibling_path'].append(sibling_prime)
    
    return result


def get_subtree_max(node):
    """Get the maximum prime value in a subtree.
    
    For leaves: return the leaf prime
    For internal nodes: return max of node prime and children maxes
    """
    if node is None:
        return 0
    
    if node.get('type') == 'leaf':
        data = node.get('data')
        if isinstance(data, list):
            return data[0] if len(data) == 1 else max(data)
        return data
    
    # Internal node
    node_prime = node.get('data', 0)
    left_max = get_subtree_max(node.get('left'))
    right_max = get_subtree_max(node.get('right'))
    return max(node_prime, left_max, right_max)


def get_subtree_min(node):
    """Get the minimum prime value in a subtree (for ordering comparison)."""
    if node is None:
        return float('inf')
    
    if node.get('type') == 'leaf':
        data = node.get('data')
        if isinstance(data, list):
            return data[0] if len(data) == 1 else min(data)
        return data
    
    # Internal node - return min of children (not the node itself for comparison)
    left_min = get_subtree_min(node.get('left'))
    right_min = get_subtree_min(node.get('right'))
    return min(left_min, right_min)


def build_merkle_tree_ordered(leaves, merkle_primes):
    """Build a Merkle tree preserving input leaf order.
    
    Properties:
    - Merkle (internal) node primes are ALL larger than ALL leaf primes (disjoint sets)
    - Input order is preserved at all levels (no reordering)
    - Parsing is deterministic because Merkle nodes are identifiable by value (> max leaf)
    
    Handles unbalanced trees: when there's an odd number of nodes at a level,
    the last node is carried up to the next level.
    
    Args:
        leaves: List of leaf primes (order preserved)
        merkle_primes: List of primes for internal nodes (sorted ascending, all > max(leaves))
    
    Returns:
        Tree root node, or None if invalid input
    """
    if not leaves:
        return None
    
    if len(leaves) == 1:
        # Single leaf - return as leaf node
        return {'type': 'leaf', 'data': leaves[0]}
    
    n_leaves = len(leaves)
    n_internal = n_leaves - 1
    
    if len(merkle_primes) < n_internal:
        raise ValueError(f"Need {n_internal} Merkle primes, but only got {len(merkle_primes)}")
    
    # Verify disjointness: all merkle primes > all leaves
    max_leaf = max(leaves)
    min_merkle = min(merkle_primes[:n_internal])
    if min_merkle <= max_leaf:
        raise ValueError(f"Merkle primes must be > max leaf. min_merkle={min_merkle}, max_leaf={max_leaf}")
    
    # Build leaf nodes (preserve input order)
    nodes = [{'type': 'leaf', 'data': p} for p in leaves]
    
    # Merkle primes: use largest for root, descending for lower levels
    # We'll assign them bottom-up, so use them in ascending order first
    merkle_primes_to_use = merkle_primes[:n_internal]
    merkle_idx = [0]  # Index into merkle_primes_to_use (ascending order)
    
    # Build tree bottom-up, preserving input order at all levels
    # Parsing is deterministic because Merkle primes are ALL > leaf primes (disjoint sets)
    # Odd node gets carried up to the next level
    while len(nodes) > 1:
        next_level = []
        i = 0
        
        while i < len(nodes):
            if i + 1 >= len(nodes):
                # Odd node - carry it up
                next_level.append(nodes[i])
                i += 1
                continue
            
            # Preserve input order (no swapping - disjoint sets make parsing deterministic)
            left = nodes[i]
            right = nodes[i + 1]
            
            # Get next Merkle prime (smallest available, working up to largest for root)
            merkle_prime = merkle_primes_to_use[merkle_idx[0]]
            merkle_idx[0] += 1
            
            parent = {
                'type': 'internal',
                'data': merkle_prime,
                'left': left,
                'right': right
            }
            next_level.append(parent)
            i += 2
        
        nodes = next_level
    
    root = nodes[0]
    
    # Reassign Merkle primes top-down so root has the LARGEST prime
    # Collect internal nodes in BFS order
    internal_nodes = []
    queue = [root]
    while queue:
        node = queue.pop(0)
        if node.get('type') == 'internal':
            internal_nodes.append(node)
            if node.get('left'):
                queue.append(node['left'])
            if node.get('right'):
                queue.append(node['right'])
    
    # Assign primes: root gets largest, then descending by BFS level
    # Reverse the merkle primes so largest is first
    merkle_primes_desc = list(reversed(merkle_primes_to_use))
    for i, node in enumerate(internal_nodes):
        node['data'] = merkle_primes_desc[i]
    
    return root


def build_merkle_tree_with_primes(leaves, pair_to_prime_dict, wordlist, wordlist_index):
    """Build a Merkle tree structure where internal nodes are primes.
    First builds the tree structure bottom-up, then assigns primes level by level
    (BFS) starting from the root (last word in wordlist), then each level left to right.
    
    Args:
        leaves: List of leaf data (each leaf is a list containing a prime)
        pair_to_prime_dict: Dictionary mapping (left_data, right_data) pairs to primes
        wordlist: List of primes from wordlist.txt (in ascending order)
        wordlist_index: List containing current index into wordlist (starting from len(wordlist)-1, working backward)
    
    Returns:
        Tuple of (tree_structure, updated_pair_to_prime_dict, max_prime_used)
    """
    if not leaves:
        return None, pair_to_prime_dict, wordlist[-1] if wordlist else 0
    
    if len(leaves) == 1:
        # Single leaf: no tree (need at least 2 leaves for a Merkle tree)
        return None, pair_to_prime_dict, wordlist[-1] if wordlist else 0
    
    # Assume even number of inputs for now
    if len(leaves) % 2 != 0:
        return None, pair_to_prime_dict, wordlist[-1] if wordlist else 0
    
    # Step 1: Build tree structure bottom-up (without assigning primes yet)
    # Start with leaf nodes
    nodes = [{'type': 'leaf', 'data': leaf} for leaf in leaves]
    tree_levels = [nodes.copy()]
    
    # Build levels bottom-up
    while len(nodes) > 1:
        next_level = []
        
        # Process nodes two at a time
        for i in range(0, len(nodes), 2):
            if i + 1 >= len(nodes):
                # Odd number - this shouldn't happen with even inputs, but handle it
                next_level.append(nodes[i])
                break
            
            left = nodes[i]
            right = nodes[i + 1]
            
            # Create parent node (without prime assignment yet)
            parent = {
                'type': 'internal',
                'data': None,  # Will assign prime later
                'left': left,
                'right': right
            }
            next_level.append(parent)
        
        tree_levels.append(next_level)
        nodes = next_level
    
    # Root is the last level's only node
    root = nodes[0] if nodes else None
    
    # Step 2: Assign primes level by level (top-down), starting from root
    # Root gets the last prime, then each level gets consecutive primes left to right
    if root is None:
        return None, pair_to_prime_dict, wordlist[-1] if wordlist else 0
    
    # Collect all internal nodes level by level (BFS from root)
    internal_nodes_by_level = []
    queue = [root]
    
    while queue:
        level_nodes = []
        next_queue = []
        
        for node in queue:
            if node.get('type') == 'internal':
                level_nodes.append(node)
                if node.get('left'):
                    next_queue.append(node['left'])
                if node.get('right'):
                    next_queue.append(node['right'])
        
        if level_nodes:
            internal_nodes_by_level.append(level_nodes)
        queue = next_queue
    
    # Step 3: Assign primes to each level, starting from root (top level)
    # Root (level 0) gets wordlist[-1], level 1 gets wordlist[-2], wordlist[-3], etc.
    max_prime_used = wordlist[-1] if wordlist else 0
    
    # First pass: assign primes level by level top-down
    for level_idx, level_nodes in enumerate(internal_nodes_by_level):
        # Assign primes left to right within each level
        for node in level_nodes:
            # Use next prime from wordlist (working backward)
            if wordlist_index[0] < 0:
                raise ValueError(f"Ran out of primes in wordlist! Need more primes for merkleization.")
            
            node_prime = wordlist[wordlist_index[0]]
            wordlist_index[0] -= 1  # Move backward through wordlist
            max_prime_used = max(max_prime_used, node_prime)
            
            # Assign prime to node
            node['data'] = node_prime
    
    # Second pass: build pair_to_prime_dict now that all nodes have primes assigned
    for level_idx, level_nodes in enumerate(internal_nodes_by_level):
        for node in level_nodes:
            # Get the data from left and right children (now they have primes assigned)
            left_data = get_node_data(node.get('left'))
            right_data = get_node_data(node.get('right'))
            
            # Extract prime numbers from data
            if isinstance(left_data, list):
                left_key = left_data[0] if len(left_data) == 1 else tuple(left_data)
            else:
                left_key = left_data  # Already a prime number
            
            if isinstance(right_data, list):
                right_key = right_data[0] if len(right_data) == 1 else tuple(right_data)
            else:
                right_key = right_data  # Already a prime number
            
            pair_key = (left_key, right_key)
            node_prime = node.get('data')
            
            # Store in dictionary
            pair_to_prime_dict[pair_key] = node_prime
    
    tree_structure = {
        'root': root,
        'levels': tree_levels,
        'num_leaves': len(leaves),
        'num_internal_nodes': len(pair_to_prime_dict)
    }
    
    return tree_structure, pair_to_prime_dict, max_prime_used


def preorder_traversal(node):
    """Generate pre-order traversal of tree (root, left, right)."""
    if node is None:
        return []
    
    result = []
    
    if node.get('type') == 'leaf':
        result.append(node.get('data'))
    elif node.get('type') == 'internal':
        result.append(node.get('data'))
        result.extend(preorder_traversal(node.get('left')))
        result.extend(preorder_traversal(node.get('right')))
    
    return result


def parse_merkleized_sequence(sequence):
    """Parse a merkleized sequence using the ordering convention.
    
    The ordering convention allows deterministic parsing:
    - Count elements: 2N-1 total → N leaves, N-1 internal nodes
    - The N-1 LARGEST primes in the sequence are Merkle (internal) nodes
    - At every internal node: left subtree < right subtree
    - Parse pre-order: root, left_subtree, right_subtree
    
    Args:
        sequence: List of primes (merkleized sequence, root first)
    
    Returns:
        Tuple of (tree_root, leaves_list, error_message)
    """
    if not sequence:
        return None, [], "Empty sequence"
    
    n_elements = len(sequence)
    
    # For a binary tree with N leaves: N-1 internal nodes, total = 2N-1
    # Solve: 2N - 1 = n_elements → N = (n_elements + 1) / 2
    if n_elements % 2 == 0:
        return None, [], f"Invalid sequence length {n_elements}: must be odd (2N-1 for N leaves)"
    
    n_leaves = (n_elements + 1) // 2
    n_merkle = n_leaves - 1
    
    # Merkle nodes are the n_merkle LARGEST primes in the sequence
    sorted_primes = sorted(sequence, reverse=True)
    merkle_set = set(sorted_primes[:n_merkle])
    
    # Verify root is the largest (first element should be in merkle_set)
    if sequence[0] not in merkle_set:
        return None, [], f"Root {sequence[0]} is not among the {n_merkle} largest primes"
    
    # Parse pre-order traversal
    idx = [0]
    leaves_found = []
    
    def parse():
        if idx[0] >= len(sequence):
            return None, "Unexpected end of sequence"
        
        p = sequence[idx[0]]
        idx[0] += 1
        
        if p in merkle_set:
            # Internal node: parse left, then right
            left, err = parse()
            if err:
                return None, err
            
            right, err = parse()
            if err:
                return None, err
            
            # No ordering check needed - disjoint sets (merkle > leaves) makes parsing deterministic
            return {'type': 'internal', 'data': p, 'left': left, 'right': right}, None
        else:
            # Leaf
            leaves_found.append(p)
            return {'type': 'leaf', 'data': p}, None
    
    tree, error = parse()
    
    if error:
        return None, [], error
    
    if idx[0] != len(sequence):
        return None, [], f"Sequence not fully consumed: parsed {idx[0]} of {len(sequence)}"
    
    return tree, leaves_found, None  # Preserve order (not sorted)


def verify_merkleized_sequence(sequence):
    """Verify a merkleized sequence is valid and canonical.
    
    Verification steps:
    1. Parse the sequence using ordering convention
    2. Extract leaves
    3. Rebuild canonical tree from leaves (using derived Merkle primes)
    4. Compare pre-order traversals
    
    Args:
        sequence: List of primes (merkleized sequence, root first)
    
    Returns:
        Tuple of (is_valid, tree, leaves, error_message)
    """
    # Step 1: Parse the sequence
    tree, leaves, error = parse_merkleized_sequence(sequence)
    
    if error:
        return False, None, [], f"Parse error: {error}"
    
    if len(leaves) < 2:
        return False, tree, leaves, f"Need at least 2 leaves, found {len(leaves)}"
    
    # Step 2: Rebuild canonical tree from leaves
    n_internal = len(leaves) - 1
    max_leaf = max(leaves)
    
    # Generate Merkle primes (n_internal primes after max_leaf)
    merkle_primes = generate_n_primes_after(max_leaf, n_internal)
    
    # Build canonical tree (preserve leaf order from parsed sequence)
    canonical_tree = build_merkle_tree_ordered(leaves, merkle_primes)
    
    if canonical_tree is None:
        return False, tree, leaves, "Failed to build canonical tree"
    
    # Step 3: Compare pre-order traversals
    original_traversal = sequence
    canonical_traversal = preorder_traversal(canonical_tree)
    
    if original_traversal != canonical_traversal:
        return False, tree, leaves, f"Non-canonical structure. Expected {canonical_traversal}, got {original_traversal}"
    
    return True, tree, leaves, "Valid canonical merkleized sequence"


def test_reconstruct_path(sequence_str):
    """Test reconstructing one path using canonical primes.
    
    Algorithm:
    - First prime is the root (highest canonical prime)
    - Generate canonical primes up to and including root: [2, 3, 5, ..., root]
    - Start with merkle_idx pointing to root (last index)
    - As we iterate:
      - If next prime == canonical_primes[merkle_idx - 1], it's a Merkle node (decrement idx)
      - Otherwise, it's a leaf (stop parsing this branch)
    
    Args:
        sequence_str: Comma-separated string of primes (merkleized sequence)
    
    Returns:
        Tuple of (path_info, error_message)
    """
    # Parse the sequence
    try:
        sequence = [int(x.strip()) for x in sequence_str.split(',')]
    except ValueError as e:
        return None, f"Failed to parse sequence: {e}"
    
    if not sequence:
        return None, "Empty sequence"
    
    # The first prime is the root (highest canonical prime)
    root_prime = sequence[0]
    
    # Generate canonical primes up to and including root
    canonical_primes = generate_primes_up_to_inclusive(root_prime)
    
    # Verify root is the last canonical prime
    if root_prime != canonical_primes[-1]:
        return None, f"Root {root_prime} is not the last prime in canonical list (last: {canonical_primes[-1]})"
    
    print(f"Canonical primes: {canonical_primes}")
    print(f"Root prime: {root_prime}")
    print(f"Sequence: {sequence}")
    
    # Start parsing from index 1 (skip root)
    sequence_idx = 1
    merkle_idx = len(canonical_primes) - 1  # Start at root position
    
    path_info = {
        'merkle_nodes': [root_prime],
        'leaves': [],
        'path': []
    }
    
    # Parse one path (leftmost path)
    while sequence_idx < len(sequence):
        current_prime = sequence[sequence_idx]
        path_info['path'].append(current_prime)
        
        # Check if this matches the next canonical prime (merkle_idx - 1)
        if merkle_idx > 0 and current_prime == canonical_primes[merkle_idx - 1]:
            # This is a Merkle node
            path_info['merkle_nodes'].append(current_prime)
            merkle_idx -= 1
            sequence_idx += 1
            print(f"  Prime {current_prime} matches canonical[{merkle_idx}] -> Merkle node (idx now {merkle_idx})")
        else:
            # This is a leaf - stop parsing this branch
            path_info['leaves'].append(current_prime)
            print(f"  Prime {current_prime} does NOT match canonical[{merkle_idx - 1}]={canonical_primes[merkle_idx - 1] if merkle_idx > 0 else 'N/A'} -> Leaf (stopping)")
            break
    
    return path_info, None


def reconstruct_and_verify(sequence_str):
    """Reconstruct a Merkle tree from a sequence.
    
    Algorithm:
    1. Parse the sequence to extract leaves (structure-based, no heuristics)
    2. Rebuild the canonical Merkle tree from those leaves using the wordlist
    3. Verify the sequence matches paths through the canonical tree
    
    Args:
        sequence_str: Comma-separated string of primes (merkleized sequence)
    
    Returns:
        Tuple of (is_valid, tree_structure, error_message)
    """
    # Parse the sequence
    try:
        sequence = [int(x.strip()) for x in sequence_str.split(',')]
    except ValueError as e:
        return False, None, f"Failed to parse sequence: {e}"
    
    if not sequence:
        return False, None, "Empty sequence"
    
    # Load wordlist first (needed for building canonical tree)
    wordlist_path = os.path.join(os.path.dirname(__file__), 'wordlist.txt')
    wordlist = load_wordlist(wordlist_path)
    
    if not wordlist:
        return False, None, "Could not load wordlist.txt"
    
    # Step 1: Parse the sequence to extract leaves (structure-based parsing)
    # The sequence follows pre-order traversal: node, left_subtree, right_subtree
    sequence_idx = [1]  # Start at index 1 (skip root)
    leaves_found = []
    
    def parse_and_collect_leaves():
        """Parse the sequence and collect leaves deterministically."""
        if sequence_idx[0] >= len(sequence):
            return None
        
        current_prime = sequence[sequence_idx[0]]
        
        # Try parsing as Merkle node first (save state for backtracking)
        saved_sequence_idx = sequence_idx[0]
        
        # Try parsing as Merkle node
        sequence_idx[0] += 1
        
        # Parse left child
        left = parse_and_collect_leaves()
        if left is None:
            # Can't parse left - backtrack and treat as leaf
            sequence_idx[0] = saved_sequence_idx
            leaf_prime = current_prime
            sequence_idx[0] += 1
            leaves_found.append(leaf_prime)
            return {'type': 'leaf', 'prime': leaf_prime}
        
        # Parse right child
        right = parse_and_collect_leaves()
        if right is None:
            # Can't parse right - backtrack and treat as leaf
            sequence_idx[0] = saved_sequence_idx
            leaf_prime = current_prime
            sequence_idx[0] += 1
            leaves_found.append(leaf_prime)
            return {'type': 'leaf', 'prime': leaf_prime}
        
        # Successfully parsed both children - this is a Merkle node
        return {'type': 'internal', 'prime': current_prime, 'left': left, 'right': right}
    
    # Parse left subtree
    left_subtree = parse_and_collect_leaves()
    if left_subtree is None:
        return False, None, f"Failed to parse left subtree at sequence_idx {sequence_idx[0]}"
    
    # Parse right subtree
    right_subtree = parse_and_collect_leaves()
    if right_subtree is None:
        return False, None, f"Failed to parse right subtree at sequence_idx {sequence_idx[0]}"
    
    if sequence_idx[0] != len(sequence):
        return False, None, f"Sequence not fully consumed. Parsed {sequence_idx[0]} of {len(sequence)} elements. Remaining: {sequence[sequence_idx[0]:]}"
    
    # Step 2: Rebuild the canonical Merkle tree from the extracted leaves
    # Sort leaves for consistency (they should be in order)
    leaves_found.sort()
    leaves = [[p] for p in leaves_found]
    
    # Check if we have enough leaves and they're even
    if len(leaves) < 2:
        return False, None, f"Need at least 2 leaves, found {len(leaves)}"
    
    if len(leaves) % 2 != 0:
        return False, None, f"Need even number of leaves, found {len(leaves)}"
    
    # Start from the last prime in wordlist and work backward
    wordlist_index = [len(wordlist) - 1]
    
    # Build canonical tree from leaves using the same process as generation
    canonical_tree, pair_to_prime_dict, max_prime_used = build_merkle_tree_with_primes(
        leaves, {}, wordlist, wordlist_index
    )
    
    if canonical_tree is None:
        return False, None, "Failed to build canonical tree from leaves"
    
    # Step 3: Verify the sequence matches the canonical tree
    # The root should match
    root_prime = sequence[0]
    canonical_root_prime = canonical_tree['root'].get('data')
    
    if root_prime != canonical_root_prime:
        return False, canonical_tree, f"Root mismatch: sequence has {root_prime}, canonical has {canonical_root_prime}"
    
    # Build the ordered list from the canonical tree (this is what the sequence should match)
    canonical_ordered_list = build_ordered_list_with_merkle_nodes(canonical_tree['root'])
    
    # Compare the sequence to the canonical ordered list
    if sequence != canonical_ordered_list:
        return False, canonical_tree, f"Sequence mismatch: expected {canonical_ordered_list}, got {sequence}"
    
    return True, canonical_tree, "Tree structure verified"


def verify_leaf_membership(sequence_str):
    """Verify membership of all leaves in a merkleized sequence.
    
    Args:
        sequence_str: Comma-separated string of primes (merkleized sequence)
    
    Returns:
        Tuple of (is_valid, proof_results, error_message)
        proof_results is a list of dicts, each containing:
            - leaf: The prime being verified
            - proof_path: List of primes in the path from root to leaf
            - sibling_path: List of sibling primes needed for verification
    """
    # Parse the sequence
    try:
        sequence = [int(x.strip()) for x in sequence_str.split(',')]
    except ValueError as e:
        return False, None, f"Failed to parse sequence: {e}"
    
    if not sequence:
        return False, None, "Empty sequence"
    
    # Reconstruct the tree
    is_valid, tree_structure, message = reconstruct_and_verify(sequence_str)
    
    if not is_valid or tree_structure is None:
        return False, None, f"Failed to reconstruct tree: {message}"
    
    root = tree_structure['root']
    
    # Extract all leaves from the tree
    def extract_all_leaves(node):
        """Extract all leaf nodes from the tree."""
        leaves = []
        if node is None:
            return leaves
        if node.get('type') == 'leaf':
            leaves.append(node)
        elif node.get('type') == 'internal':
            if node.get('left'):
                leaves.extend(extract_all_leaves(node['left']))
            if node.get('right'):
                leaves.extend(extract_all_leaves(node['right']))
        return leaves
    
    all_leaf_nodes = extract_all_leaves(root)
    
    # Build proof results for each leaf
    proof_results = []
    
    for leaf_node in all_leaf_nodes:
        # Extract the prime from the leaf
        leaf_data = get_node_data(leaf_node)
        if isinstance(leaf_data, list):
            leaf_prime = leaf_data[0] if len(leaf_data) == 1 else leaf_data
        else:
            leaf_prime = leaf_data
        
        result = {
            'leaf': leaf_prime,
            'proof_path': [],
            'sibling_path': []
        }
        
        # Get the path from root to this leaf
        path = get_merkle_proof_path(root, leaf_node)
        
        if path:
            # Extract primes from the path
            # For leaves, extract the prime from the list; for internal nodes, use the prime directly
            proof_path_primes = []
            for node in path:
                node_data = get_node_data(node)
                if isinstance(node_data, list):
                    # Leaf node - extract the prime from the list
                    prime = node_data[0] if len(node_data) == 1 else node_data
                else:
                    # Internal node - use the prime directly
                    prime = node_data
                proof_path_primes.append(prime)
            result['proof_path'] = proof_path_primes
            
            # Build sibling path (siblings needed for verification)
            # For each internal node in the path (except the last), get its sibling
            for i in range(len(path) - 1):
                current_node = path[i]
                next_node = path[i + 1]
                
                # Determine if next_node is left or right child
                if current_node.get('left') == next_node:
                    # Next is left child, so sibling is right
                    sibling = current_node.get('right')
                elif current_node.get('right') == next_node:
                    # Next is right child, so sibling is left
                    sibling = current_node.get('left')
                else:
                    sibling = None
                
                if sibling:
                    sibling_data = get_node_data(sibling)
                    # Extract prime from sibling data (could be list for leaf, or prime for internal)
                    if isinstance(sibling_data, list):
                        sibling_prime = sibling_data[0] if len(sibling_data) == 1 else sibling_data
                    else:
                        sibling_prime = sibling_data
                    result['sibling_path'].append(sibling_prime)
        
        proof_results.append(result)
    
    return True, proof_results, None


def merkleize_primes(input_primes):
    """Merkleize a list of primes, preserving input order.
    
    The disjoint sets convention:
    - All Merkle (internal) node primes are > all leaf primes
    - Input order is preserved in the output
    - Parsing is deterministic because Merkle nodes are identifiable by value
    
    Args:
        input_primes: List of primes to merkleize (order preserved)
    
    Returns:
        Tuple of (merkleized_sequence, tree, merkle_primes, error)
    """
    if not input_primes:
        return None, None, None, "Empty input"
    
    if len(input_primes) == 1:
        return input_primes, None, [], None  # Single element, no tree needed
    
    # Use input primes as leaves (tree builder will order canonically)
    leaves = list(input_primes)
    n = len(leaves)
    
    # Generate Merkle primes: n-1 primes after max(leaves)
    max_leaf = max(leaves)
    n_internal = n - 1
    merkle_primes = generate_n_primes_after(max_leaf, n_internal)
    
    # Build tree with ordering convention
    tree = build_merkle_tree_ordered(leaves, merkle_primes)
    
    if tree is None:
        return None, None, None, "Failed to build tree"
    
    # Generate pre-order traversal (root first)
    sequence = preorder_traversal(tree)
    
    return sequence, tree, merkle_primes, None


def main():
    parser = argparse.ArgumentParser(
        description='Merkleize primes using the ordering convention (left < right at every node)',
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  merkelize_primes.py "2,5,11,23"                    # Merkleize → outputs sequence
  merkelize_primes.py "2,5,11,23" -H                 # Highlight Merkle primes (green)
  echo "2,5,11,23" | merkelize_primes.py             # Merkleize from stdin
  merkelize_primes.py "2,5,11,23" | merkelize_primes.py --verify  # Round-trip
  merkelize_primes.py "2,5,11,23" -v                 # Verbose: show tree
  merkelize_primes.py -p "37,29,2,5,31,11,23"        # Parse → outputs leaves
  merkelize_primes.py --verify "37,29,2,5,31,11,23"  # Verify → "valid" or error
  merkelize_primes.py -N 4 --seed 42                 # Random primes

The ordering convention:
  - Merkle (internal) node primes are LARGER than all leaf primes
  - At every internal node: left child < right child
  - This makes parsing deterministic without side bits
        """
    )
    parser.add_argument('primes', type=str, nargs='?', help='Primes to merkleize (comma-separated, or reads from stdin)')
    parser.add_argument('-N', '--random', type=int, metavar='N', help='Generate N random primes instead of reading input')
    parser.add_argument('--seed', type=int, default=0, help='Random seed for -N (default: 0)')
    parser.add_argument('--parse', '-p', type=str, nargs='?', const='', help='Parse a merkleized sequence (from arg or stdin)')
    parser.add_argument('--verify', type=str, nargs='?', const='', help='Verify a merkleized sequence is canonical (from arg or stdin)')
    parser.add_argument('--proof', type=str, help='Generate membership proofs for all leaves (comma-separated primes)')
    parser.add_argument('--verbose', '-v', action='store_true', help='Show detailed tree visualization')
    parser.add_argument('--highlight', '-H', action='store_true', help='Highlight Merkle primes in output (green)')
    parser.add_argument('--test-path', type=str, help='Test path reconstruction (legacy)')
    
    args = parser.parse_args()
    
    # ANSI color codes
    MERKLE_COLOR = '\033[92m'  # Green
    LEAF_COLOR = '\033[94m'    # Blue
    RESET_COLOR = '\033[0m'
    
    # Handle --parse mode
    if args.parse is not None:
        # Read from argument or stdin
        parse_input = args.parse if args.parse else None
        if not parse_input and not sys.stdin.isatty():
            parse_input = sys.stdin.read().strip()
        
        if not parse_input:
            print("Error: --parse requires input (argument or stdin)", file=sys.stderr)
            sys.exit(1)
        
        try:
            sequence = [int(x.strip()) for x in parse_input.split(',')]
        except ValueError as e:
            print(f"Error: Failed to parse sequence: {e}", file=sys.stderr)
            sys.exit(1)
        
        tree, leaves, error = parse_merkleized_sequence(sequence)
        
        if error:
            print(f"Error: {error}", file=sys.stderr)
            sys.exit(1)
        
        # Quiet mode: just output leaves
        if not args.verbose:
            print(','.join(str(p) for p in leaves))
            sys.exit(0)
        
        # Verbose mode
        print("\nParsed Merkleized Sequence:")
        print("=" * 60)
        print(f"Input sequence: {sequence}")
        print(f"Extracted leaves: {LEAF_COLOR}{leaves}{RESET_COLOR}")
        print(f"Root: {MERKLE_COLOR}{sequence[0]}{RESET_COLOR}")
        print(f"Number of leaves: {len(leaves)}")
        print(f"Number of Merkle nodes: {len(leaves) - 1}")
        print("\nTree Structure:")
        draw_tree_visualization(tree)
        print("=" * 60)
        sys.exit(0)
    
    # Handle --verify mode
    if args.verify is not None:
        # Read from argument or stdin
        verify_input = args.verify if args.verify else None
        if not verify_input and not sys.stdin.isatty():
            verify_input = sys.stdin.read().strip()
        
        if not verify_input:
            print("Error: --verify requires input (argument or stdin)", file=sys.stderr)
            sys.exit(1)
        
        try:
            sequence = [int(x.strip()) for x in verify_input.split(',')]
        except ValueError as e:
            print(f"Error: Failed to parse sequence: {e}", file=sys.stderr)
            sys.exit(1)
        
        is_valid, tree, leaves, message = verify_merkleized_sequence(sequence)
        
        # Quiet mode: just exit code (0=valid, 1=invalid)
        if not args.verbose:
            if is_valid:
                print("valid")
            else:
                print(f"invalid: {message}", file=sys.stderr)
                sys.exit(1)
            sys.exit(0)
        
        # Verbose mode
        print("\nVerification Result:")
        print("=" * 60)
        print(f"Input sequence: {sequence}")
        
        if tree:
            print(f"\nExtracted leaves: {LEAF_COLOR}{leaves}{RESET_COLOR}")
            print("\nTree Structure:")
            draw_tree_visualization(tree)
        
        if is_valid:
            print(f"\n{MERKLE_COLOR}✓ {message}{RESET_COLOR}")
        else:
            print(f"\n\033[91m✗ {message}{RESET_COLOR}", file=sys.stderr)
            sys.exit(1)
        
        print("=" * 60)
        sys.exit(0)
    
    # Handle --proof mode
    if args.proof:
        is_valid, proof_results, error = verify_leaf_membership(args.proof)
        
        if error:
            print(f"Error: {error}", file=sys.stderr)
            sys.exit(1)
        
        if not is_valid or not proof_results:
            print("Error: No leaves found in the sequence", file=sys.stderr)
            sys.exit(1)
        
        print("\nLeaf Membership Proofs:")
        print("=" * 60)
        
        for result in proof_results:
            print(f"\nLeaf: {LEAF_COLOR}{result['leaf']}{RESET_COLOR}")
            print(f"  Proof path (root to leaf): {result['proof_path']}")
            print(f"  Sibling path (for verification): {result['sibling_path']}")
        
        print("\n" + "=" * 60)
        print(f"{MERKLE_COLOR}✓ Generated {len(proof_results)} proof(s){RESET_COLOR}")
        sys.exit(0)
    
    # Handle --test-path mode (legacy)
    if args.test_path:
        path_info, error = test_reconstruct_path(args.test_path)
        
        if error:
            print(f"Error: {error}", file=sys.stderr)
            sys.exit(1)
        
        print("\nPath Reconstruction Test:")
        print("=" * 60)
        print(f"Merkle nodes found: {path_info['merkle_nodes']}")
        print(f"Leaves found: {path_info['leaves']}")
        print(f"Path: {path_info['path']}")
        print("=" * 60)
        sys.exit(0)
    
    # Determine input primes
    input_primes = None
    
    if args.random is not None:
        # Generate N random primes
        if args.random < 2:
            print("Error: N must be at least 2", file=sys.stderr)
            sys.exit(1)
        
        # Set random seed
        random.seed(args.seed)
        
        # Generate a pool of primes to choose from
        max_prime_candidate = 7919  # ~1000th prime
        prime_pool = generate_primes_up_to_inclusive(max_prime_candidate)
        
        # Extend if needed
        while len(prime_pool) < args.random:
            num = prime_pool[-1] + 1
            if is_prime(num):
                prime_pool.append(num)
        
        # Select N random primes
        input_primes = random.sample(prime_pool, args.random)
    elif args.primes:
        # Parse comma-separated input from argument
        try:
            input_primes = [int(x.strip()) for x in args.primes.split(',')]
        except ValueError as e:
            print(f"Error: Failed to parse input primes: {e}", file=sys.stderr)
            sys.exit(1)
    elif not sys.stdin.isatty():
        # Read from stdin
        try:
            stdin_data = sys.stdin.read().strip()
            if stdin_data:
                input_primes = [int(x.strip()) for x in stdin_data.split(',')]
        except ValueError as e:
            print(f"Error: Failed to parse stdin: {e}", file=sys.stderr)
            sys.exit(1)
    
    if not input_primes:
        print("Error: Provide primes to merkleize (as argument, stdin, or use -N for random)")
        print("       Use --parse or --verify to process a merkleized sequence")
        sys.exit(1)
    
    # Filter to only primes (silently ignore non-primes)
    input_primes = [p for p in input_primes if is_prime(p)]
    
    if len(input_primes) == 0:
        print("Error: No primes found in input", file=sys.stderr)
        sys.exit(1)
    
    # Load wordlist for validation
    wordlist_path = os.path.join(os.path.dirname(__file__), 'wordlist.txt')
    wordlist = load_wordlist(wordlist_path)
    wordlist_set = set(wordlist) if wordlist else set()
    
    # Validate all input primes are in the canonical wordlist
    if wordlist_set:
        invalid_primes = [p for p in input_primes if p not in wordlist_set]
        if invalid_primes:
            print(f"Error: Primes not in canonical wordlist: {invalid_primes}", file=sys.stderr)
            print(f"       Wordlist contains primes up to {max(wordlist)}", file=sys.stderr)
            sys.exit(1)
    
    if len(input_primes) == 1:
        # Single prime - nothing to merkelize, just output it
        print(input_primes[0])
        sys.exit(0)
    
    # Merkleize the primes (preserves input order at leaf level)
    sequence, tree, merkle_primes, error = merkleize_primes(input_primes)
    
    if error:
        print(f"Error: {error}", file=sys.stderr)
        sys.exit(1)
    
    # Auto-verify: catch bugs early by verifying the generated sequence
    if sequence and len(sequence) > 1:
        is_valid, _, parsed_leaves, verify_msg = verify_merkleized_sequence(sequence)
        if not is_valid:
            print(f"BUG: Generated sequence failed verification: {verify_msg}", file=sys.stderr)
            print(f"     Sequence: {sequence}", file=sys.stderr)
            sys.exit(1)
        # Also verify leaf order matches input
        if parsed_leaves != input_primes:
            print(f"BUG: Leaf order not preserved!", file=sys.stderr)
            print(f"     Input:  {input_primes}", file=sys.stderr)
            print(f"     Output: {parsed_leaves}", file=sys.stderr)
            sys.exit(1)
    
    # Extract output leaves (for verbose display)
    output_leaves = None
    if merkle_primes:
        merkle_set = set(merkle_primes)
        output_leaves = [p for p in sequence if p not in merkle_set]
    
    # Default: quiet output (just the sequence)
    if not args.verbose:
        if merkle_primes:
            # Highlight Merkle primes in green
            merkle_set = set(merkle_primes)
            parts = []
            for p in sequence:
                if p in merkle_set:
                    parts.append(f"{MERKLE_COLOR}{p}{RESET_COLOR}")
                else:
                    parts.append(str(p))
            print(','.join(parts))
        else:
            print(','.join(str(p) for p in sequence))
        sys.exit(0)
    
    # Verbose output
    print(f"\nInput primes (leaves): {LEAF_COLOR}{input_primes}{RESET_COLOR}")
    
    if tree is None:
        # Single leaf case
        print(f"\nMerkleized sequence: {sequence}")
        sys.exit(0)
    
    print(f"Merkle primes (internal nodes): {MERKLE_COLOR}{merkle_primes}{RESET_COLOR}")
    print(f"Root: {MERKLE_COLOR}{sequence[0]}{RESET_COLOR}")
    
    # Render tree
    print("\nMerkle Tree Structure:")
    print("=" * 60)
    draw_tree_visualization(tree)
    print("=" * 60)
    
    # Output merkleized sequence with coloring
    merkle_set = set(merkle_primes)
    
    print("\nMerkleized Sequence (pre-order traversal):")
    print("=" * 60)
    for i, p in enumerate(sequence):
        if p in merkle_set:
            print(f"{MERKLE_COLOR}{p}{RESET_COLOR}", end="")
        else:
            print(f"{LEAF_COLOR}{p}{RESET_COLOR}", end="")
        if i < len(sequence) - 1:
            print(", ", end="")
    print()
    print("=" * 60)
    
    # Plain sequence for copy-paste
    print(f"\nPlain sequence: {','.join(str(p) for p in sequence)}")
    
    # Verification hint
    print(f"\nTo verify: merkelize_primes.py --verify \"{','.join(str(p) for p in sequence)}\"")
    print(f"To parse:  merkelize_primes.py -p \"{','.join(str(p) for p in sequence)}\"")


if __name__ == "__main__":
    main()
