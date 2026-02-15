#!/usr/bin/env python3
"""
Generate a large list of primes and save to wordlist.txt
"""

import math


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
    """Generate all primes up to and including max_value."""
    primes = []
    num = 2
    
    while num <= max_value:
        if is_prime(num):
            primes.append(num)
        num += 1
    
    return primes


def main():
    # Generate primes up to 100000 (or generate first N primes)
    max_prime = 100000
    
    print(f"Generating primes up to {max_prime}...")
    primes = generate_primes_up_to(max_prime)
    
    print(f"Generated {len(primes)} primes")
    print(f"First 10: {primes[:10]}")
    print(f"Last 10: {primes[-10:]}")
    
    # Save to wordlist.txt
    with open('wordlist.txt', 'w') as f:
        for prime in primes:
            f.write(str(prime) + '\n')
    
    print(f"Saved {len(primes)} primes to wordlist.txt")


if __name__ == "__main__":
    main()
