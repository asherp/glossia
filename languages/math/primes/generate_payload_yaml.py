#!/usr/bin/env python3
"""
Generate payload.yaml from wordlist.txt for math/primes language.
All primes are tagged as Det (determiners) since they represent leaf nodes.
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

def generate_payload_yaml(wordlist_path, output_path):
    """Generate payload.yaml from wordlist.txt."""
    primes = []
    
    # Read primes from wordlist
    with open(wordlist_path, 'r') as f:
        for line in f:
            line = line.strip()
            if line:
                try:
                    prime = int(line)
                    if is_prime(prime):
                        primes.append(prime)
                except ValueError:
                    continue
    
    # Write payload.yaml
    with open(output_path, 'w') as f:
        f.write("# Payload wordlist for math/primes language\n")
        f.write("# All primes are tagged as Det (determiners) since they represent leaf nodes\n")
        f.write("# Generated from wordlist.txt\n\n")
        
        for prime in primes:
            f.write(f"{prime}:\n")
            f.write("  Det: 1.0\n\n")
    
    print(f"Generated payload.yaml with {len(primes)} primes", file=sys.stderr)

if __name__ == "__main__":
    import os
    
    script_dir = os.path.dirname(os.path.abspath(__file__))
    wordlist_path = os.path.join(script_dir, "wordlist.txt")
    output_path = os.path.join(script_dir, "payload.yaml")
    
    generate_payload_yaml(wordlist_path, output_path)
    print(f"Created: {output_path}")
