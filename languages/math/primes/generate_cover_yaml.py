#!/usr/bin/env python3
"""
Generate cover.yaml for math/primes language.
Cover words are non-prime integers that can be placed between primes.
"""

import sys

def is_prime(n):
    """Check if a number is prime."""
    if n < 2:
        return False
    if n == 2:
        return True
    if n % 2 == 0:
        return False
    i = 3
    while i * i <= n:
        if n % i == 0:
            return False
        i += 2
    return True

def generate_cover_yaml(output_path, max_value=1000):
    """Generate cover.yaml with non-prime integers."""
    cover_words = []
    
    # Generate non-prime integers up to max_value
    for n in range(1, max_value + 1):
        if not is_prime(n):
            cover_words.append(n)
    
    # Write cover.yaml
    with open(output_path, 'w') as f:
        f.write("# Cover words for math/primes language\n")
        f.write("# Non-prime integers that can be placed between primes\n")
        f.write("# These satisfy the constraint: left_prime < cover_word < right_prime\n\n")
        
        # Group by POS - use N, V, Det for different types
        # For simplicity, assign all to multiple POS tags
        for word in cover_words:
            f.write(f"{word}:\n")
            # Assign to N, V, and Det so they can fill any slot
            f.write("  N: 0.4\n")
            f.write("  V: 0.4\n")
            f.write("  Det: 0.2\n\n")
    
    print(f"Generated cover.yaml with {len(cover_words)} non-prime integers (1-{max_value})", file=sys.stderr)

if __name__ == "__main__":
    import os
    
    script_dir = os.path.dirname(os.path.abspath(__file__))
    output_path = os.path.join(script_dir, "cover.yaml")
    
    generate_cover_yaml(output_path, max_value=1000)
    print(f"Created: {output_path}")
