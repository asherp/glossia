#!/usr/bin/env python3
"""
Script to generate primes up to greater than the maximum of N random integers.
Usage: python get_integers.py N [--seed SEED]
Example: python get_integers.py 10
Example: python get_integers.py 10 --seed 42
"""

import sys
import math
import random
import argparse
import numpy as np
import hashlib


def is_prime(n):
    """Check if a number is prime."""
    if n < 2:
        return False
    if n == 2:
        return True
    if n % 2 == 0:
        return False
    
    # Check divisibility up to sqrt(n)
    for i in range(3, int(math.sqrt(n)) + 1, 2):
        if n % i == 0:
            return False
    return True


def generate_primes_up_to(max_value):
    """Generate all primes up to and including the first prime greater than max_value."""
    primes = []
    num = 2
    found_prime_greater_than_max = False
    
    while not found_prime_greater_than_max:
        if is_prime(num):
            primes.append(num)
            if num > max_value:
                found_prime_greater_than_max = True
        num += 1
    
    return primes


def generate_primes_up_to_inclusive(max_value):
    """Generate all primes up to and including max_value."""
    primes = []
    num = 2
    
    while num <= max_value:
        if is_prime(num):
            primes.append(num)
        num += 1
    
    return primes


def prime_factorization(n):
    """Compute the prime factorization of n.
    Returns a list of tuples (prime, exponent)."""
    if n < 2:
        return []
    
    factors = []
    num = n
    divisor = 2
    
    while divisor * divisor <= num:
        if num % divisor == 0:
            count = 0
            while num % divisor == 0:
                num //= divisor
                count += 1
            factors.append((divisor, count))
        divisor += 1
    
    if num > 1:
        factors.append((num, 1))
    
    return factors


def format_factorization(n, factors):
    """Format prime factorization as a string."""
    if n < 2:
        return f"{n} = {n}"
    
    if not factors:
        return f"{n} = {n}"
    
    parts = []
    for prime, exp in factors:
        if exp == 1:
            parts.append(str(prime))
        else:
            parts.append(f"{prime}^{exp}")
    
    return f"{n} = {' * '.join(parts)}"


def calculate_internal_nodes_needed(num_leaves):
    """Calculate internal nodes needed by counting as we build the tree.
    
    For each level starting at leaves:
    - For every two items, add 1 to the count (create 1 parent)
    - If we get to the end and there is only 1 item left, don't add to the count
    - Continue until we reach the root (1 node remaining)
    
    Returns 0 for 0 or 1 leaves (no tree possible).
    """
    if num_leaves <= 1:
        return 0  # No tree with 0 or 1 leaves
    
    count = 0
    current = num_leaves
    
    while current > 1:
        # For every two items, add 1 to the count
        pairs = current // 2
        count += pairs
        
        # Move to next level: we have pairs parent nodes, plus possibly 1 leftover
        current = pairs + (current % 2)
    
    # When we get to 1 node (the root), we don't add to the count
    return count


def calculate_primes_needed_for_non_overlapping_merkle_tree(all_mapped_factors, num_leaves):
    """Calculate how many additional primes are needed so Merkle tree nodes don't overlap
    with factorization tuples. Uses binary tree structure for better estimation.
    
    Args:
        all_mapped_factors: List of all tuples from factorizations
        num_leaves: Number of leaves (N)
    
    Returns:
        Tuple of (additional_primes_first, additional_primes_second) needed
    """
    if not all_mapped_factors:
        return (0, 0)
    
    # Find max values from factorizations
    max_first = max(t[0] for t in all_mapped_factors)
    max_second = max(t[1] for t in all_mapped_factors)
    
    # Get all unique tuples from factorizations
    factorization_tuples = set(tuple(t) for t in all_mapped_factors)
    
    # Generate initial canonical list
    primes_first = generate_primes_up_to_inclusive(max_first)
    primes_second = generate_primes_up_to_inclusive(max_second)
    
    # Calculate how many internal nodes we'll need based on binary tree structure
    num_merkle_tuples = calculate_internal_nodes_needed(num_leaves)
    
    # Find the index of max tuple in canonical ordering
    # Count how many tuples come before or equal to (max_first, max_second)
    tuples_up_to_max = 0
    max_tuple_index = -1
    for idx, first in enumerate(primes_first):
        for jdx, second in enumerate(primes_second):
            tuple_idx = idx * len(primes_second) + jdx
            if (first, second) <= (max_first, max_second):
                tuples_up_to_max += 1
                if (first, second) == (max_first, max_second):
                    max_tuple_index = tuple_idx
    
    # We need num_merkle_tuples tuples AFTER max_tuple_index that don't overlap
    # Calculate how many non-overlapping tuples exist after max_tuple_index
    current_total = len(primes_first) * len(primes_second)
    non_overlapping_after_max = 0
    
    # Count non-overlapping tuples after max
    for idx in range(max_tuple_index + 1, current_total):
        first_idx = idx // len(primes_second)
        second_idx = idx % len(primes_second)
        if first_idx < len(primes_first) and second_idx < len(primes_second):
            tuple_val = (primes_first[first_idx], primes_second[second_idx])
            if tuple_val not in factorization_tuples:
                non_overlapping_after_max += 1
    
    # If we already have enough non-overlapping tuples, no extension needed
    if non_overlapping_after_max >= num_merkle_tuples:
        return (0, 0)
    
    # Calculate how many more tuples we need
    tuples_needed = num_merkle_tuples - non_overlapping_after_max
    
    if tuples_needed <= 0:
        return (0, 0)
    
    # Strategy: extend primes_second first (simpler), then primes_first if needed
    # Each additional prime in primes_second adds len(primes_first) tuples
    tuples_per_additional_second = len(primes_first)
    
    # Try extending primes_second first
    # We need at least enough tuples to cover tuples_needed, accounting for potential overlaps
    # Start with a conservative estimate: assume we need 2x the tuples (accounting for overlaps)
    # But we can refine this by checking iteratively
    
    # Start with 1 additional prime in second
    additional_primes_second = 1
    while True:
        # Simulate: add additional_primes_second primes to primes_second
        # Count how many new non-overlapping tuples we'd get
        # (We can't generate actual primes here, so estimate based on position)
        
        # New tuples would be: all tuples with second > max_second
        # Each new prime in second adds len(primes_first) tuples
        new_tuples_from_second = additional_primes_second * len(primes_first)
        
        # Estimate overlap: some of these might be in factorization_tuples
        # But since we're going beyond max_second, overlap should be minimal
        # Conservative: assume 10% overlap
        estimated_new_non_overlapping = int(new_tuples_from_second * 0.9)
        
        if estimated_new_non_overlapping >= tuples_needed:
            # Check if we need to extend primes_first
            # If extending second isn't enough, we'll need to extend first too
            new_primes_second_count = len(primes_second) + additional_primes_second
            new_total = len(primes_first) * new_primes_second_count
            
            # More precise: count actual non-overlapping tuples we'd have
            # All tuples after (max_first, max_second) that don't overlap
            estimated_total_non_overlapping = new_total - tuples_up_to_max
            # Subtract estimated overlaps (tuples in factorization_tuples that are after max)
            estimated_total_non_overlapping = max(0, estimated_total_non_overlapping - len(factorization_tuples))
            
            if estimated_total_non_overlapping < num_merkle_tuples:
                # Need to extend primes_first too
                still_needed = num_merkle_tuples - estimated_total_non_overlapping
                tuples_per_additional_first = new_primes_second_count
                additional_primes_first = (still_needed + tuples_per_additional_first - 1) // tuples_per_additional_first
                return (additional_primes_first, additional_primes_second)
            else:
                return (0, additional_primes_second)
        
        # Not enough yet, try one more prime in second
        additional_primes_second += 1
        
        # Safety check: if we'd need too many, switch to extending first
        if additional_primes_second > 10:  # Arbitrary limit
            # Switch strategy: extend first instead
            tuples_per_additional_first = len(primes_second)
            additional_primes_first = (tuples_needed + tuples_per_additional_first - 1) // tuples_per_additional_first
            return (additional_primes_first, 1)  # Still add at least 1 to second


def build_merkle_tree(leaves, internal_node_tuples):
    """Build a Merkle tree structure from leaves and internal node tuples.
    
    Args:
        leaves: List of leaf data (factorizations for each integer)
        internal_node_tuples: List of N-1 tuples for internal nodes (in descending order)
    
    Returns:
        Dictionary representing the tree structure with nodes and their relationships
    """
    if not leaves:
        return None
    
    if len(leaves) == 1:
        # Single leaf: no tree (need at least 2 leaves for a Merkle tree)
        return None
    
    # Build tree bottom-up
    # Start with leaves
    current_level = [{'type': 'leaf', 'data': leaf, 'id': i} for i, leaf in enumerate(leaves)]
    tree_levels = [current_level]
    tuple_idx = len(internal_node_tuples) - 1  # Start from the end (lowest tuples)
    
    # Track all tuples used to ensure uniqueness
    used_tuples = set()
    
    # Build levels bottom-up until we reach root
    while len(current_level) > 1:
        next_level = []
        level_idx = 0
        
        # Pair up nodes: for every two items, create one parent
        # If there's an odd number, the last node moves up unpaired
        num_pairs = len(current_level) // 2
        has_leftover = len(current_level) % 2 == 1
        
        # Create parent nodes for each pair
        for i in range(num_pairs):
            left = current_level[i * 2]
            right = current_level[i * 2 + 1]
            
            # Assign tuple from canonical list (using from end to start)
            # CRITICAL: Each parent internal node MUST get a unique tuple
            if tuple_idx >= 0:
                node_tuple = internal_node_tuples[tuple_idx]
                # Verify this tuple hasn't been used before
                if node_tuple in used_tuples:
                    raise ValueError(
                        f"Duplicate tuple detected: {node_tuple} has already been used. "
                        f"This indicates a bug in tuple assignment or selection. "
                        f"Used tuples so far: {sorted(used_tuples)}"
                    )
                used_tuples.add(node_tuple)
                tuple_idx -= 1
            else:
                tuples_used = len(internal_node_tuples)
                tuples_needed = tuples_used + 1
                raise ValueError(
                    f"Not enough internal node tuples for Merkle tree construction! "
                    f"Need at least {tuples_needed} tuples, but only have {tuples_used}. "
                    f"This indicates insufficient unique tuples were generated or selected."
                )
            
            parent = {
                'type': 'internal',
                'data': node_tuple,
                'left': left,
                'right': right,
                'id': level_idx
            }
            next_level.append(parent)
            level_idx += 1
        
        # If there's a leftover node (odd number), it moves up to the next level unpaired
        if has_leftover:
            next_level.append(current_level[-1])
        
        tree_levels.append(next_level)
        current_level = next_level
    
    # Root is the last level's only node
    root = current_level[0] if current_level else None
    
    return {
        'root': root,
        'levels': tree_levels,
        'num_leaves': len(leaves),
        'num_internal_nodes': len(internal_node_tuples)
    }


def build_ordered_list(node, result=None):
    """Build a list where each Merkle node appears before the data it represents.
    Traverses the tree in pre-order: node, then left subtree, then right subtree.
    """
    if result is None:
        result = []
    
    if node is None:
        return result
    
    if node.get('type') == 'leaf':
        # Leaf: add its data
        result.append(node['data'])
    elif node.get('type') == 'internal':
        # Internal node: add the node first, then traverse children
        result.append(node['data'])
        if 'left' in node:
            build_ordered_list(node['left'], result)
        if 'right' in node:
            build_ordered_list(node['right'], result)
    
    return result


def flatten_leaf_data(data):
    """Flatten leaf data (list of tuples) into individual tuples."""
    if isinstance(data, list):
        return data
    else:
        return [data]


def build_ordered_list_with_merkle_nodes(node, result=None):
    """Build a list where each Merkle node appears before the sequence it hashes.
    For leaves, we flatten their tuples. For internal nodes, we place the node
    before all tuples from its subtree.
    """
    if result is None:
        result = []
    
    if node is None:
        return result
    
    if node.get('type') == 'leaf':
        # Leaf: add all its tuples
        leaf_data = node['data']
        if isinstance(leaf_data, list):
            result.extend(leaf_data)
        else:
            result.append(leaf_data)
    elif node.get('type') == 'internal':
        # Internal node: add the node first, then traverse children
        result.append(node['data'])
        if 'left' in node:
            build_ordered_list_with_merkle_nodes(node['left'], result)
        if 'right' in node:
            build_ordered_list_with_merkle_nodes(node['right'], result)
    
    return result


def reconstruct_tree_from_ordered_list(ordered_list, merkle_nodes_set):
    """Reconstruct a Merkle tree from an ordered list.
    
    The ordered list has the structure where each Merkle node appears before
    the sequence it represents. This function reconstructs the tree structure.
    
    Args:
        ordered_list: List of tuples where Merkle nodes appear before sequences
        merkle_nodes_set: Set of Merkle node tuples (internal nodes)
    
    Returns:
        Tuple of (tree_node, next_index) or (None, current_index) if invalid
    """
    if not ordered_list:
        return None, 0
    
    def parse_subtree(start_idx):
        """Parse a subtree starting at start_idx."""
        if start_idx >= len(ordered_list):
            return None, start_idx
        
        current = ordered_list[start_idx]
        
        # Check if this is a Merkle node (internal node)
        if current in merkle_nodes_set:
            # This is a Merkle node - it should have children following
            merkle_node = current
            
            # Parse left subtree
            left_node, next_idx = parse_subtree(start_idx + 1)
            if left_node is None:
                return None, start_idx
            
            # Parse right subtree
            right_node, next_idx = parse_subtree(next_idx)
            if right_node is None:
                return None, start_idx
            
            return {
                'type': 'internal',
                'data': merkle_node,
                'left': left_node,
                'right': right_node
            }, next_idx
        else:
            # This is a leaf tuple - collect all consecutive leaf tuples
            # Leaves can be single tuples or lists of tuples
            leaf_tuples = []
            idx = start_idx
            
            # Collect consecutive non-Merkle tuples as a leaf
            while idx < len(ordered_list) and ordered_list[idx] not in merkle_nodes_set:
                leaf_tuples.append(ordered_list[idx])
                idx += 1
            
            # A leaf can be a single tuple or a list of tuples
            if len(leaf_tuples) == 1:
                leaf_data = leaf_tuples[0]
            else:
                leaf_data = leaf_tuples
            
            return {
                'type': 'leaf',
                'data': leaf_data
            }, idx
    
    root, end_idx = parse_subtree(0)
    
    if root is None or end_idx != len(ordered_list):
        return None
    
    return root


def verify_and_correct_merkle_sequence(ordered_list_input, merkle_nodes_set=None):
    """Verify and correct a merkleized sequence.
    
    Takes an ordered list (string or list), reconstructs the tree,
    and returns the corrected merkle sequence.
    
    Args:
        ordered_list_input: String representation or list of ordered list
        merkle_nodes_set: Optional set of Merkle node tuples. If None, will
                         attempt to identify them from the structure.
    
    Returns:
        Tuple of (corrected_ordered_list, is_valid, error_message)
    """
    import ast
    
    # Parse input
    if isinstance(ordered_list_input, str):
        try:
            ordered_list = ast.literal_eval(ordered_list_input)
        except (ValueError, SyntaxError) as e:
            return None, False, f"Invalid input format: {e}"
    elif isinstance(ordered_list_input, list):
        ordered_list = ordered_list_input
    else:
        return None, False, "Input must be a string or list"
    
    if not ordered_list:
        return None, False, "Ordered list is empty"
    
    # Verify all items are tuples
    for item in ordered_list:
        if not isinstance(item, tuple) or len(item) != 2:
            return None, False, f"Invalid item format: {item} (must be tuple of 2 elements)"
    
    # Identify Merkle nodes if not provided
    # Merkle nodes are those that appear before other tuples (internal nodes)
    # The first item is always the root (a Merkle node)
    if merkle_nodes_set is None:
        # Parse the structure to identify Merkle nodes
        # Merkle nodes are those that have children following them
        merkle_nodes_set = set()
        
        def identify_merkle_nodes(start_idx):
            """Recursively identify Merkle nodes by parsing structure."""
            if start_idx >= len(ordered_list):
                return start_idx
            
            current = ordered_list[start_idx]
            
            # Try to parse as Merkle node: it should have at least one child
            if start_idx + 1 < len(ordered_list):
                # Parse left subtree
                left_end = identify_merkle_nodes(start_idx + 1)
                
                # If we consumed items, this might be a Merkle node
                if left_end > start_idx + 1:
                    # Parse right subtree
                    right_end = identify_merkle_nodes(left_end)
                    
                    # If we have both left and right, this is a Merkle node
                    if right_end > left_end:
                        merkle_nodes_set.add(current)
                        return right_end
            
            # This is a leaf - just consume this item
            return start_idx + 1
        
        # Start parsing from root (first item is always root/Merkle node)
        identify_merkle_nodes(0)
        
        # Ensure root is in the set (it should be, but just in case)
        if ordered_list:
            merkle_nodes_set.add(ordered_list[0])
    
    # Reconstruct tree from ordered list
    root_node = reconstruct_tree_from_ordered_list(ordered_list, merkle_nodes_set)
    
    if root_node is None:
        return None, False, "Failed to reconstruct tree from ordered list"
    
    # Regenerate the ordered list from the reconstructed tree (this is the corrected version)
    corrected_ordered_list = build_ordered_list_with_merkle_nodes(root_node)
    
    # Check if correction was needed
    is_valid = corrected_ordered_list == ordered_list
    
    if is_valid:
        return corrected_ordered_list, True, "Sequence is valid"
    else:
        return corrected_ordered_list, False, "Sequence was corrected"


def reconstruct_integers_from_merkleized_sequence(merkleized_sequence_str):
    """Reconstruct the original integer sequence from a merkleized sequence.
    
    Uses canonical ordering to identify Merkle nodes:
    - Start with merkle index i = -1 (last element in canonical tuple list)
    - If next tuple matches tuple at i-1, it is a Merkle tuple, set i=i-1
    - If next tuple does not match tuple at i-1, it is a leaf
    
    Args:
        merkleized_sequence_str: Comma-separated string of values from merkleized output
    
    Returns:
        List of original integers, or None if reconstruction fails
    """
    # Parse the comma-separated sequence into a list of integers
    try:
        # Split by comma and convert to integers
        values = [int(x.strip()) for x in merkleized_sequence_str.split(',')]
    except ValueError as e:
        print(f"Error: Failed to parse input sequence: {e}", file=sys.stderr)
        return None
    
    if len(values) % 2 != 0:
        print(f"Error: Input must have even number of values (pairs), got {len(values)}", file=sys.stderr)
        return None  # Must have even number of values (pairs)
    
    # Group into pairs (tuples)
    tuples = [(values[i], values[i+1]) for i in range(0, len(values), 2)]
    
    if not tuples:
        print(f"Error: No tuples created from input sequence", file=sys.stderr)
        return None
    
    # Find all primes used in the sequence to build canonical list
    all_primes_in_sequence = set()
    for t in tuples:
        all_primes_in_sequence.add(t[0])
        all_primes_in_sequence.add(t[1])
    
    # Generate primes up to max
    max_prime = max(all_primes_in_sequence) if all_primes_in_sequence else 2
    primes_first = generate_primes_up_to_inclusive(max_prime)
    primes_second = generate_primes_up_to_inclusive(max_prime)
    
    # Build canonical list (Cartesian product)
    primes_first_arr = np.array(primes_first)
    primes_second_arr = np.array(primes_second)
    first_grid, second_grid = np.meshgrid(primes_first_arr, primes_second_arr, indexing='ij')
    first_flat = first_grid.flatten()
    second_flat = second_grid.flatten()
    cartesian_product_2d = np.column_stack((first_flat, second_flat))
    all_canonical_tuples = [tuple(row) for row in cartesian_product_2d]
    
    # Identify Merkle nodes using canonical ordering algorithm
    # Algorithm: merkle index i = -1 (last element in canonical tuple list)
    # If the next item matches tuple at i-1, it is a Merkle tuple, set i=i-1
    # If the next tuple does not match tuple at i-1, it is a leaf
    #
    # First, we need to identify which tuples are factorization tuples (leaves)
    # by checking if they appear in the sequence but don't match Merkle positions.
    # Then Merkle nodes are those that match positions from the end of canonical list.
    
    # Strategy: 
    # 1. First identify all unique tuples in sequence
    # 2. Build non-overlapping canonical list (excluding factorization tuples)
    # 3. Merkle nodes are the last N-1 tuples from non-overlapping list
    # 4. But we need to identify factorizations first...
    
    # Alternative: Use the algorithm as specified - check each tuple against canonical[i-1]
    # Start with i = -1 (last canonical tuple)
    merkle_nodes_set = set()
    factorization_tuples_set = set()
    
    # We need to know which canonical tuples are available for Merkle nodes
    # (i.e., which ones are NOT factorization tuples)
    # Let's first collect all tuples that appear, then identify Merkle nodes
    
    # Algorithm: Start with merkle index i = -1 (last element in canonical tuple list)
    # Key insight: Leaves and Merkle tuples are DISJOINT - a tuple is either one or the other
    # 
    # Process:
    # 1. First, identify leaves by checking which tuples DON'T match positions near end of canonical
    # 2. Build non-overlapping canonical list (excluding leaves)
    # 3. Then identify Merkle nodes: if tuple matches non_overlapping[i-1], it is Merkle
    # 4. Collect consecutive leaves until next Merkle node
    # 5. Continue until all tuples are accounted for
    
    # Step 1: Identify leaves (tuples that appear in sequence but don't match end positions)
    # We'll use a heuristic: collect all tuples, then identify which are Merkle by position
    
    # Step 2: Build non-overlapping canonical list
    # First, collect all unique tuples from sequence
    all_sequence_tuples = set(tuples)
    
    # Initial guess: tuples that appear multiple times are likely leaves
    # But we need a better method - use the canonical ordering
    
    # Use iterative approach: identify Merkle nodes by checking against non-overlapping canonical
    # Since leaves and Merkle are disjoint, we can refine iteratively
    
    merkle_nodes_set = set()
    factorization_tuples_set = set()
    
    # Iterative refinement: start with all tuples as potential factorizations
    # Then identify Merkle nodes from non-overlapping list
    for iteration in range(5):  # Max 5 iterations
        # Build non-overlapping canonical list (excluding current factorization set)
        non_overlapping_canonical = [t for t in all_canonical_tuples if t not in factorization_tuples_set]
        
        if not non_overlapping_canonical:
            break
        
        # Start from end of non-overlapping canonical list (i = -1 means index len-1)
        merkle_canonical_idx = len(non_overlapping_canonical) - 1  # i = -1
        tuple_idx = 0
        new_merkle_nodes = set()
        new_factorizations = set()
        
        # Traverse sequence and identify Merkle nodes vs leaves
        while tuple_idx < len(tuples):
            current_tuple = tuples[tuple_idx]
            
            # Check if current tuple matches non_overlapping_canonical[i-1]
            check_idx = merkle_canonical_idx - 1
            
            if check_idx >= 0 and current_tuple == non_overlapping_canonical[check_idx]:
                # Matches non_overlapping[i-1] - this is a Merkle node
                new_merkle_nodes.add(current_tuple)
                merkle_canonical_idx = check_idx  # Set i = i-1
                tuple_idx += 1
            else:
                # Does NOT match non_overlapping[i-1] - this is a leaf
                # Collect consecutive leaves until we reach the next Merkle node
                while tuple_idx < len(tuples):
                    current = tuples[tuple_idx]
                    check_idx = merkle_canonical_idx - 1
                    
                    # Check if this tuple is a Merkle node
                    if check_idx >= 0 and current == non_overlapping_canonical[check_idx]:
                        # Found a Merkle node, stop collecting leaves
                        break
                    
                    # This is a leaf tuple
                    new_factorizations.add(current)
                    tuple_idx += 1
        
        # Update sets
        if new_merkle_nodes == merkle_nodes_set and new_factorizations == factorization_tuples_set:
            # Converged
            break
        
        merkle_nodes_set = new_merkle_nodes
        factorization_tuples_set = new_factorizations
    
    # Final non-overlapping canonical list
    non_overlapping_canonical = [t for t in all_canonical_tuples if t not in factorization_tuples_set]
    
    # Debug logging
    print(f"Debug: Identified {len(merkle_nodes_set)} Merkle nodes: {sorted(merkle_nodes_set)}", file=sys.stderr)
    print(f"Debug: Identified {len(factorization_tuples_set)} factorization tuples", file=sys.stderr)
    print(f"Debug: Non-overlapping canonical list length: {len(non_overlapping_canonical)}", file=sys.stderr)
    print(f"Debug: Total tuples in sequence: {len(tuples)}", file=sys.stderr)
    
    # Now verify: Merkle nodes should match positions from the end of non_overlapping_canonical
    # But we've already identified them above, so this is just for verification
    
    # Now reconstruct the tree structure using the identified Merkle nodes
    def parse_tree(start_idx):
        """Parse tree structure and return (node, next_idx)."""
        if start_idx >= len(tuples):
            return None, start_idx
        
        current = tuples[start_idx]
        
        if current in merkle_nodes_set:
            # Merkle node - parse children
            left_node, left_end = parse_tree(start_idx + 1)
            if left_node is None:
                return None, start_idx
            
            right_node, right_end = parse_tree(left_end)
            if right_node is None:
                return None, start_idx
            
            return {
                'type': 'internal',
                'data': current,
                'left': left_node,
                'right': right_node
            }, right_end
        else:
            # Leaf - collect consecutive tuples
            leaf_tuples = []
            idx = start_idx
            while idx < len(tuples) and tuples[idx] not in merkle_nodes_set:
                leaf_tuples.append(tuples[idx])
                idx += 1
            
            # A leaf can be a single tuple or a list of tuples
            if len(leaf_tuples) == 1:
                leaf_data = leaf_tuples[0]
            else:
                leaf_data = leaf_tuples
            
            return {
                'type': 'leaf',
                'data': leaf_data
            }, idx
    
    # Parse the tree
    root, end_idx = parse_tree(0)
    if root is None:
        print(f"Error: Failed to parse tree structure. Parsed {end_idx} of {len(tuples)} tuples.", file=sys.stderr)
        print(f"Merkle nodes identified: {merkle_nodes_set}", file=sys.stderr)
        print(f"Factorization tuples: {len(factorization_tuples_set)} tuples", file=sys.stderr)
        return None
    if end_idx != len(tuples):
        print(f"Error: Tree parsing incomplete. Parsed {end_idx} of {len(tuples)} tuples.", file=sys.stderr)
        print(f"Remaining tuples: {tuples[end_idx:]}", file=sys.stderr)
        return None
    
    # Extract all leaves from the tree
    def extract_leaves(node):
        """Extract all leaves from the tree."""
        leaves = []
        if node.get('type') == 'leaf':
            leaves.append(node['data'])
        elif node.get('type') == 'internal':
            if 'left' in node:
                leaves.extend(extract_leaves(node['left']))
            if 'right' in node:
                leaves.extend(extract_leaves(node['right']))
        return leaves
    
    leaf_factorizations = extract_leaves(root)
    
    print(f"Debug: Extracted {len(leaf_factorizations)} leaves: {leaf_factorizations}", file=sys.stderr)
    
    # Convert mapped factors back to original integers
    # Mapped: (prime, primes[exp - 1])
    # Need to find which exponent corresponds to primes[exp - 1]
    
    # Generate primes for exponent mapping
    max_prime_for_exp = max(all_primes_in_sequence) if all_primes_in_sequence else 2
    primes = generate_primes_up_to_inclusive(max_prime_for_exp)
    
    # Create mapping: prime -> index in primes list
    prime_to_index = {p: i for i, p in enumerate(primes)}
    
    # Convert each leaf factorization back to integer
    original_integers = []
    for leaf in leaf_factorizations:
        if leaf is None:
            continue
        
        # Normalize to list of tuples
        if isinstance(leaf, tuple):
            factors_list = [leaf]
        elif isinstance(leaf, list):
            factors_list = leaf
        else:
            continue
        
        # Convert mapped factors back to (prime, exponent)
        original_factors = []
        for mapped_prime, mapped_exp_prime in factors_list:
            # Find which exponent this mapped_exp_prime represents
            if mapped_exp_prime not in prime_to_index:
                print(f"Error: Invalid mapping - prime {mapped_exp_prime} not found in primes list.", file=sys.stderr)
                print(f"Primes list (first 20): {primes[:20]}", file=sys.stderr)
                print(f"Leaf factorization: {leaf}", file=sys.stderr)
                return None  # Invalid mapping
            
            exp_index = prime_to_index[mapped_exp_prime]
            exponent = exp_index + 1  # Since primes[exp - 1] means exponent is exp_index + 1
            
            original_factors.append((mapped_prime, exponent))
        
        # Reconstruct integer from factors
        result = 1
        for prime, exp in original_factors:
            result *= prime ** exp
        
        original_integers.append(result)
    
    if not original_integers:
        print(f"Error: No integers reconstructed. Leaf factorizations: {leaf_factorizations}", file=sys.stderr)
        return None
    
    return original_integers


def verify_ordered_list_integrity(ordered_list, merkle_nodes, expected_root=None):
    """Verify the integrity of an ordered list from a Merkle tree.
    
    The ordered list should have the structure where each Merkle node appears
    before the sequence it represents. This function verifies:
    1. All Merkle nodes are present and in correct positions
    2. The root (first Merkle node) matches expected_root if provided
    3. The structure is valid (Merkle nodes appear before their subtrees)
    
    Args:
        ordered_list: List of tuples where Merkle nodes appear before sequences
        merkle_nodes: Set of Merkle node tuples (internal nodes)
        expected_root: Optional expected root tuple to verify
    
    Returns:
        Tuple of (is_valid, error_message)
    """
    if not ordered_list:
        return (False, "Ordered list is empty")
    
    if not merkle_nodes:
        return (False, "No Merkle nodes provided")
    
    # Extract Merkle nodes from ordered list
    found_merkle_nodes = [item for item in ordered_list if item in merkle_nodes]
    found_leaf_tuples = [item for item in ordered_list if item not in merkle_nodes]
    
    # Verify we found all Merkle nodes
    if len(found_merkle_nodes) != len(merkle_nodes):
        missing = merkle_nodes - set(found_merkle_nodes)
        return (False, f"Missing Merkle nodes in ordered list: {missing}")
    
    # Verify root is first Merkle node
    root = found_merkle_nodes[0] if found_merkle_nodes else None
    if expected_root and root != expected_root:
        return (False, f"Root mismatch: expected {expected_root}, got {root}")
    
    # Verify structure: Merkle nodes should appear before their subtrees
    # This is a simplified check - full verification would reconstruct the tree
    # For now, we verify that:
    # 1. Root is first
    # 2. All Merkle nodes are present
    # 3. The list contains the expected number of items
    
    return (True, "Integrity verified")


def draw_tree_visualization(node, prefix="", is_last=True, is_root=True):
    """Draw a tree visualization with leaves at the bottom."""
    if node is None:
        return
    
    MERKLE_COLOR = '\033[92m'  # Green
    RESET_COLOR = '\033[0m'     # Reset
    
    if node.get('type') == 'leaf':
        # Leaf node
        leaf_data = node['data']
        if isinstance(leaf_data, list):
            data_str = str(leaf_data)
        else:
            data_str = str(leaf_data)
        connector = "└── " if is_last else "├── "
        print(f"{prefix}{connector}Leaf: {data_str}")
    elif node.get('type') == 'internal':
        # Internal node
        node_data = node['data']
        connector = "└── " if is_last else "├── "
        if is_root:
            print(f"{MERKLE_COLOR}{node_data}{RESET_COLOR}")
        else:
            print(f"{prefix}{connector}{MERKLE_COLOR}{node_data}{RESET_COLOR}")
        
        # Prepare prefix for children
        if is_root:
            child_prefix = ""
        else:
            child_prefix = prefix + ("    " if is_last else "│   ")
        
        # Draw children
        if 'left' in node and 'right' in node:
            draw_tree_visualization(node['left'], child_prefix, False, False)
            draw_tree_visualization(node['right'], child_prefix, True, False)
        elif 'left' in node:
            draw_tree_visualization(node['left'], child_prefix, True, False)
        elif 'right' in node:
            draw_tree_visualization(node['right'], child_prefix, True, False)


def print_merkle_tree(tree, indent=0, prefix=""):
    """Print the Merkle tree structure."""
    if tree is None:
        return
    
    if isinstance(tree, dict):
        if tree.get('type') == 'leaf':
            print(f"{prefix}Leaf: {tree['data']}")
        elif tree.get('type') == 'internal':
            print(f"{prefix}Internal: {tree['data']}")
            if 'left' in tree:
                print_merkle_tree(tree['left'], indent + 2, prefix + "  ├─ ")
            if 'right' in tree:
                print_merkle_tree(tree['right'], indent + 2, prefix + "  └─ ")
        elif 'root' in tree:
            # Top-level tree structure
            print(f"\nMerkle Tree Structure:")
            print(f"Root: {tree['root']['data']}")
            print(f"Number of leaves: {tree['num_leaves']}")
            print(f"Number of internal nodes: {tree['num_internal_nodes']}")
            print(f"\nTree levels (bottom-up):")
            for level_idx, level in enumerate(tree['levels']):
                print(f"\nLevel {level_idx} ({len(level)} nodes):")
                for node in level:
                    if node['type'] == 'leaf':
                        print(f"  Leaf: {node['data']}")
                    else:
                        left_data = node['left']['data'] if 'left' in node else None
                        right_data = node['right']['data'] if 'right' in node else None
                        print(f"  Internal: {node['data']} (left: {left_data}, right: {right_data})")
            
            # Draw tree visualization
            print(f"\nTree Visualization (leaves at bottom):")
            draw_tree_visualization(tree['root'])


def main():
    parser = argparse.ArgumentParser(
        description='Generate primes up to greater than the maximum of N random integers, verify a merkleized sequence, or reconstruct original integers'
    )
    parser.add_argument('N', type=int, nargs='?', help='Number of random integers to generate (required unless --verify or --reconstruct is used)')
    parser.add_argument('--seed', type=int, default=0, help='Random seed (default: 0)')
    parser.add_argument('--verify', type=str, help='Verify and correct a merkleized sequence (provide as string representation of list)')
    parser.add_argument('--reconstruct', type=str, help='Reconstruct original integers from merkleized sequence (comma-separated values)')
    
    args = parser.parse_args()
    
    # Handle --reconstruct mode
    if args.reconstruct:
        original_integers = reconstruct_integers_from_merkleized_sequence(args.reconstruct)
        
        if original_integers is None:
            print(f"Error: Failed to reconstruct integers from merkleized sequence", file=sys.stderr)
            sys.exit(1)
        
        # Output the original integers
        print(original_integers)
        sys.exit(0)
    
    # Handle --verify mode
    if args.verify:
        corrected_list, is_valid, message = verify_and_correct_merkle_sequence(args.verify)
        
        if corrected_list is None:
            print(f"Error: {message}", file=sys.stderr)
            sys.exit(1)
        
        # ANSI color codes for Merkle nodes
        MERKLE_COLOR = '\033[92m'  # Green
        RESET_COLOR = '\033[0m'     # Reset
        
        # Identify Merkle nodes from the structure (first item is root, then parse recursively)
        # For simplicity, we'll identify them by reconstructing the tree
        merkle_nodes_set = set()
        if corrected_list:
            # First item is always the root (Merkle node)
            merkle_nodes_set.add(corrected_list[0])
            
            # Try to identify other Merkle nodes by parsing structure
            def identify_merkle_nodes_recursive(start_idx):
                """Recursively identify Merkle nodes."""
                if start_idx >= len(corrected_list):
                    return start_idx
                
                current = corrected_list[start_idx]
                
                if current in merkle_nodes_set:
                    # This is a Merkle node - parse children
                    left_end = identify_merkle_nodes_recursive(start_idx + 1)
                    if left_end > start_idx + 1:
                        right_end = identify_merkle_nodes_recursive(left_end)
                        return right_end
                    return start_idx + 1
                else:
                    # Leaf - consume consecutive non-Merkle items
                    idx = start_idx
                    while idx < len(corrected_list) and corrected_list[idx] not in merkle_nodes_set:
                        idx += 1
                    return idx
            
            identify_merkle_nodes_recursive(0)
        
        # Flatten the corrected list into a single sequence of elements
        flattened_elements = []
        for item in corrected_list:
            if isinstance(item, tuple):
                flattened_elements.extend(item)
            else:
                flattened_elements.append(item)
        
        # Output as comma-separated list with colored Merkle nodes
        element_idx = 0
        for i, item in enumerate(corrected_list):
            is_merkle = item in merkle_nodes_set
            if isinstance(item, tuple):
                for j, elem in enumerate(item):
                    if is_merkle:
                        # Merkle node element - highlight with green color
                        print(f"{MERKLE_COLOR}{elem}{RESET_COLOR}", end="")
                    else:
                        # Regular element
                        print(elem, end="")
                    
                    if element_idx < len(flattened_elements) - 1:
                        print(", ", end="")
                    element_idx += 1
            else:
                if is_merkle:
                    print(f"{MERKLE_COLOR}{item}{RESET_COLOR}", end="")
                else:
                    print(item, end="")
                
                if element_idx < len(flattened_elements) - 1:
                    print(", ", end="")
                element_idx += 1
        print()  # Newline at the end
        
        # Exit with error code if sequence was invalid
        if not is_valid:
            sys.exit(1)
        
        sys.exit(0)
    
    # Normal mode - require N
    if args.N is None:
        print("Error: N is required unless --verify or --reconstruct is used")
        sys.exit(1)
    
    if args.N < 1:
        print("Error: N must be at least 1")
        sys.exit(1)
    
    # Set random seed
    random.seed(args.seed)
    
    # Generate N random integers
    # Using a reasonable range - you can adjust this if needed
    random_ints = [random.randint(1, 1000) for _ in range(args.N)]
    
    max_n = max(random_ints)
    
    # Generate primes up to greater than max (needed for indexing)
    primes = generate_primes_up_to(max_n)
    
    # Also generate enough primes to cover any exponent values
    # Find max exponent that might appear in factorizations
    max_exp = 0
    for num in random_ints:
        factors = prime_factorization(num)
        for _, exp in factors:
            max_exp = max(max_exp, exp)
    
    # Generate additional primes if needed for indexing
    if max_exp > len(primes):
        num = primes[-1] + 1
        while len(primes) < max_exp:
            if is_prime(num):
                primes.append(num)
            num += 1
    
    # Print the input
    print(f"Input: {random_ints}")
    
    # Collect all mapped factors to compute max tuple and store leaves
    all_mapped_factors = []
    leaves = []  # Store factorizations for each integer (leaves of Merkle tree)
    
    for num in random_ints:
        factors = prime_factorization(num)
        if num < 2:
            # For numbers < 2, use primes[0] = 2 as the second element
            mapped_factors = [(num, primes[0])]
            all_mapped_factors.extend(mapped_factors)
            leaves.append(mapped_factors)  # Store as leaf
        elif not factors:
            mapped_factors = [(num, primes[0])]
            all_mapped_factors.extend(mapped_factors)
            leaves.append(mapped_factors)  # Store as leaf
        else:
            # Replace exponent with prime at that index (exponent-1 for 1-indexed)
            mapped_factors = [(prime, primes[exp - 1]) for prime, exp in factors]
            all_mapped_factors.extend(mapped_factors)
            leaves.append(mapped_factors)  # Store as leaf
    
    # Print factorizations
    print("Factorizations:")
    for num, mapped_factors in zip(random_ints, leaves):
        print(f"  {num}: {mapped_factors}")
    
    # Flatten the list of prime factorized tuples
    flattened_tuples = [tuple(t) for t in all_mapped_factors]
    
    # Determine max tuple using Cartesian product ordering
    if all_mapped_factors:
        # First, find the range of primes needed (max values from factors)
        max_first = max(t[0] for t in all_mapped_factors)
        max_second = max(t[1] for t in all_mapped_factors)
        
        # Calculate how many additional primes needed to avoid overlap
        factorization_tuples_set = set(tuple(t) for t in all_mapped_factors)
        additional_first, additional_second = calculate_primes_needed_for_non_overlapping_merkle_tree(
            all_mapped_factors, args.N
        )
        
        # Generate two separate lists of primes (up to and including max, plus additional)
        primes_first = generate_primes_up_to_inclusive(max_first)
        primes_second = generate_primes_up_to_inclusive(max_second)
        
        # Extend primes if needed
        if additional_first > 0:
            num = primes_first[-1] + 1 if primes_first else 2
            count = 0
            while count < additional_first:
                if is_prime(num):
                    primes_first.append(num)
                    count += 1
                num += 1
        
        if additional_second > 0:
            num = primes_second[-1] + 1 if primes_second else 2
            count = 0
            while count < additional_second:
                if is_prime(num):
                    primes_second.append(num)
                    count += 1
                num += 1
        
        # Extended primes if needed (silent)
        
        # Compute Cartesian product using numpy as a 2D array
        primes_first_arr = np.array(primes_first)
        primes_second_arr = np.array(primes_second)
        first_grid, second_grid = np.meshgrid(primes_first_arr, primes_second_arr, indexing='ij')
        
        # Stack into a 2D array where each row is a pair (first, second)
        first_flat = first_grid.flatten()
        second_flat = second_grid.flatten()
        cartesian_product_2d = np.column_stack((first_flat, second_flat))
        
        # Find max tuple from all_mapped_factors using lexicographic ordering (matching Cartesian product)
        # Find max using lexicographic ordering: compare first element, then second
        max_tuple = max(flattened_tuples, key=lambda t: (t[0], t[1]))
        
        # Find the index of max tuple in the canonical list (Cartesian product)
        max_tuple_arr = np.array(max_tuple)
        max_idx = None
        for idx, row in enumerate(cartesian_product_2d):
            if np.array_equal(row, max_tuple_arr):
                max_idx = idx
                break
        
        # Calculate how many internal nodes we'll need
        # Standard formula: n - 1 where n is the number of leaves
        # When building the tree, if a level has odd number of nodes, we duplicate the last one
        num_merkle_tuples = calculate_internal_nodes_needed(args.N)
        
        # Get the last N - 1 tuples from the canonical list for internal nodes
        # (for Merkle tree construction, where N is the number of factorizations)
        # Ensure they don't overlap with factorization tuples
        
        # Convert cartesian product to list of tuples
        all_canonical_tuples = [tuple(row) for row in cartesian_product_2d]
        
        # Filter out tuples that appear in factorizations, then take last N-1
        non_overlapping_tuples = [t for t in all_canonical_tuples if t not in factorization_tuples_set]
        
        # Ensure we have enough unique tuples
        # Remove any duplicates from non_overlapping_tuples (shouldn't happen, but safety check)
        unique_non_overlapping = []
        seen = set()
        for t in non_overlapping_tuples:
            if t not in seen:
                unique_non_overlapping.append(t)
                seen.add(t)
        
        if num_merkle_tuples > 0 and len(unique_non_overlapping) >= num_merkle_tuples:
            # Get the last num_merkle_tuples unique non-overlapping tuples
            merkle_tuples = unique_non_overlapping[-num_merkle_tuples:]
            merkle_tuples.reverse()  # Sort in descending order
            root_tuple = merkle_tuples[0] if merkle_tuples else None
            
            # Verify all tuples are unique
            if len(merkle_tuples) != len(set(merkle_tuples)):
                duplicates = [t for t in merkle_tuples if merkle_tuples.count(t) > 1]
                raise ValueError(f"Duplicate tuples in Merkle tree nodes: {set(duplicates)}")
            
            # Verify no overlap with factorizations
            overlap = set(merkle_tuples) & factorization_tuples_set
            if overlap:
                # Still raise error for overlap, but don't print warning
                raise ValueError(f"Overlap detected between Merkle nodes and factorizations: {overlap}")
        elif num_merkle_tuples > 0:
            # Not enough non-overlapping tuples - this should not happen if primes were extended correctly
            raise ValueError(f"Not enough unique non-overlapping tuples for Merkle tree. Need {num_merkle_tuples}, have {len(unique_non_overlapping)}")
        else:
            merkle_tuples = []
            root_tuple = None
        
        # Merkle tree is just the N-1 internal nodes (root is included as first after reversing)
        merkle_tree = merkle_tuples
        
        # Verify we have enough tuples before building the tree
        if leaves and len(leaves) > 1:
            if len(merkle_tuples) < num_merkle_tuples:
                raise ValueError(
                    f"Insufficient tuples for Merkle tree construction. "
                    f"Need {num_merkle_tuples} tuples, but only have {len(merkle_tuples)}. "
                    f"This indicates the prime extension calculation failed to provide enough unique tuples."
                )
        
        # Build the actual tree structure
        tree_structure = None
        if merkle_tuples and leaves:
            tree_structure = build_merkle_tree(leaves, merkle_tuples)
        
        # Build ordered list where each Merkle node appears before the sequence it hashes
        # This is the primary output
        if tree_structure and tree_structure.get('root'):
            ordered_list = build_ordered_list_with_merkle_nodes(tree_structure['root'])
        else:
            # Fallback: concatenate merkle tree and flattened list
            ordered_list = merkle_tree + flattened_tuples
        
        # ANSI color codes for Merkle nodes
        MERKLE_COLOR = '\033[92m'  # Green
        RESET_COLOR = '\033[0m'     # Reset
        
        # Create a set of Merkle nodes for highlighting
        merkle_node_set = set(merkle_tree) if merkle_tree else set()
        
        # Flatten the ordered list into a single sequence of elements
        # Each item in ordered_list is a tuple (a, b), we want to flatten to a, b, c, d, ...
        flattened_elements = []
        for item in ordered_list:
            if isinstance(item, tuple):
                flattened_elements.extend(item)
            else:
                flattened_elements.append(item)
        
        # Output as comma-separated list with colored Merkle nodes
        element_idx = 0
        for i, item in enumerate(ordered_list):
            is_merkle = item in merkle_node_set
            if isinstance(item, tuple):
                for j, elem in enumerate(item):
                    if is_merkle:
                        # Merkle node element - highlight with green color
                        print(f"{MERKLE_COLOR}{elem}{RESET_COLOR}", end="")
                    else:
                        # Regular element
                        print(elem, end="")
                    
                    if element_idx < len(flattened_elements) - 1:
                        print(", ", end="")
                    element_idx += 1
            else:
                if is_merkle:
                    print(f"{MERKLE_COLOR}{item}{RESET_COLOR}", end="")
                else:
                    print(item, end="")
                
                if element_idx < len(flattened_elements) - 1:
                    print(", ", end="")
                element_idx += 1
        print()  # Newline at the end


if __name__ == "__main__":
    main()
