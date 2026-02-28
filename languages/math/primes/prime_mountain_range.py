#!/usr/bin/env python3
"""
Prime Mountain Range (PMR): An append-optimized Merkle structure for primes.

A PMR is a list of perfect binary tree "peaks" whose sizes match the set bits
of N (the leaf count).  Appending a leaf costs O(log N) worst case, O(1)
amortized -- vs O(N) for the flat rebuild in merkelize_primes.py.

Key properties:
- Leaves and Merkle primes are drawn from disjoint ranges of a shared sieve
- Merkle primes are allocated in ascending order (bottom-up), so each peak's
  root is its largest Merkle prime
- The next leaf prime is always sieve[leaf_cursor] -- O(1), no primality test
- Serialization is self-describing: sequence length L = 2N - popcount(N)

Usage:
  python prime_mountain_range.py "2,3,5,7,11"       # Append 5 primes, show peaks
  python prime_mountain_range.py -N 16               # Power-of-2 case
  python prime_mountain_range.py -p "..."             # Parse serialized PMR
  python prime_mountain_range.py --verify "..."       # Round-trip verify
  python prime_mountain_range.py --bench 10000        # Benchmark
  python prime_mountain_range.py --test               # Run all tests
"""

import sys
import os
import math
import time
import argparse

# Import from get_integers.py
sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..', 'integers'))
from get_integers import is_prime, generate_primes_up_to_inclusive


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def popcount(n):
    """Count the number of set bits in n."""
    count = 0
    while n:
        count += n & 1
        n >>= 1
    return count


def sieve_primes(bound):
    """Generate all primes up to bound using Sieve of Eratosthenes."""
    if bound < 2:
        return []
    is_p = [True] * (bound + 1)
    is_p[0] = is_p[1] = False
    for i in range(2, int(math.sqrt(bound)) + 1):
        if is_p[i]:
            for j in range(i * i, bound + 1, i):
                is_p[j] = False
    return [i for i in range(2, bound + 1) if is_p[i]]


def find_N_from_length(L):
    """Solve N from L = 2N - popcount(N) via binary search.

    f(N) = 2N - popcount(N) is strictly increasing, so binary search works.
    Returns N or None if no solution exists.
    """
    if L == 0:
        return 0
    if L == 1:
        return 1

    lo, hi = 1, L  # N <= L always
    while lo < hi:
        mid = (lo + hi) // 2
        val = 2 * mid - popcount(mid)
        if val < L:
            lo = mid + 1
        else:
            hi = mid

    if 2 * lo - popcount(lo) == L:
        return lo
    return None


def generate_n_primes_after(start, n):
    """Generate n primes strictly greater than start."""
    primes = []
    candidate = start + 1
    while len(primes) < n:
        if is_prime(candidate):
            primes.append(candidate)
        candidate += 1
    return primes


# ---------------------------------------------------------------------------
# Peak node
# ---------------------------------------------------------------------------

class PeakNode:
    """A node in a peak (perfect binary tree)."""
    __slots__ = ('prime', 'left', 'right', 'height')

    def __init__(self, prime, left=None, right=None, height=0):
        self.prime = prime
        self.left = left
        self.right = right
        self.height = height

    def is_leaf(self):
        return self.left is None and self.right is None

    def preorder(self):
        """Pre-order traversal: root, left, right."""
        result = [self.prime]
        if self.left:
            result.extend(self.left.preorder())
        if self.right:
            result.extend(self.right.preorder())
        return result

    def leaves_in_order(self):
        """In-order leaf traversal."""
        if self.is_leaf():
            return [self.prime]
        result = []
        if self.left:
            result.extend(self.left.leaves_in_order())
        if self.right:
            result.extend(self.right.leaves_in_order())
        return result

    def merkle_primes(self):
        """Collect all Merkle (internal) primes."""
        if self.is_leaf():
            return []
        result = [self.prime]
        if self.left:
            result.extend(self.left.merkle_primes())
        if self.right:
            result.extend(self.right.merkle_primes())
        return result

    def __repr__(self):
        if self.is_leaf():
            return f"Leaf({self.prime})"
        return f"Internal({self.prime}, h={self.height})"


# ---------------------------------------------------------------------------
# Prime Mountain Range
# ---------------------------------------------------------------------------

class PrimeMountainRange:
    """Append-optimized Merkle structure using binary carry merges.

    Leaves and Merkle primes occupy disjoint ranges of a shared sieve.
    Appending a leaf is O(1) amortized, O(log N) worst case.
    """

    def __init__(self, sieve_bound=1000):
        self.n_leaves = 0
        self.leaf_primes = []
        self.peaks = []  # list of PeakNode, tallest to shortest

        # Sieve management
        self._sieve_bound = sieve_bound
        self._sieve = sieve_primes(sieve_bound)

        # Split allocation: leaves from bottom, Merkle from midpoint upward
        self._merkle_boundary = len(self._sieve) // 2
        self._leaf_cursor = 0
        self._merkle_cursor = self._merkle_boundary

    def _extend_sieve(self):
        """Double sieve bound and recompute, preserving cursor positions.

        After extension, the leaf range must not overlap with old Merkle
        allocations.  We jump the leaf cursor past the old Merkle high-water
        mark and place the new Merkle boundary at the midpoint of the
        remaining range.
        """
        old_merkle_hwm = self._merkle_cursor  # highest Merkle index consumed

        new_bound = self._sieve_bound * 2
        new_sieve = sieve_primes(new_bound)

        self._sieve_bound = new_bound
        self._sieve = new_sieve

        # Leaf cursor jumps past any old Merkle region to avoid overlap
        self._leaf_cursor = max(self._leaf_cursor, old_merkle_hwm)

        # Place new boundary at the midpoint of the remaining range
        remaining = len(new_sieve) - self._leaf_cursor
        self._merkle_boundary = self._leaf_cursor + remaining // 2
        self._merkle_cursor = self._merkle_boundary

    def _next_leaf_prime(self):
        """Get next leaf prime from sieve (lower half). Extends if needed."""
        while self._leaf_cursor >= self._merkle_boundary:
            self._extend_sieve()
        p = self._sieve[self._leaf_cursor]
        self._leaf_cursor += 1
        return p

    def _next_merkle_prime(self):
        """Get next Merkle prime from sieve (upper half, ascending). Extends if needed."""
        while self._merkle_cursor >= len(self._sieve):
            self._extend_sieve()
        p = self._sieve[self._merkle_cursor]
        self._merkle_cursor += 1
        return p

    def peek_next_leaf(self):
        """Preview the next leaf prime without consuming it. O(1)."""
        while self._leaf_cursor >= self._merkle_boundary:
            self._extend_sieve()
        return self._sieve[self._leaf_cursor]

    def append(self, prime=None):
        """Append a leaf. If prime is None, use next from sieve.

        Returns the appended prime.
        """
        if prime is not None:
            leaf = prime
            # If this happens to be the next sieve prime, advance cursor
            if (self._leaf_cursor < self._merkle_boundary and
                    self._sieve[self._leaf_cursor] == prime):
                self._leaf_cursor += 1
        else:
            leaf = self._next_leaf_prime()

        self.n_leaves += 1
        self.leaf_primes.append(leaf)

        # New singleton peak (height 0)
        self.peaks.append(PeakNode(leaf, height=0))

        # Merge while last two peaks have equal height (binary carry)
        while (len(self.peaks) >= 2 and
               self.peaks[-1].height == self.peaks[-2].height):
            right = self.peaks.pop()
            left = self.peaks.pop()
            m = self._next_merkle_prime()
            merged = PeakNode(m, left, right, height=left.height + 1)
            self.peaks.append(merged)

        return leaf

    def serialize(self):
        """Serialize PMR as concatenated pre-order traversals of peaks (L->R)."""
        result = []
        for peak in self.peaks:
            result.extend(peak.preorder())
        return result

    def all_leaves(self):
        """Get all leaves in order across all peaks."""
        result = []
        for peak in self.peaks:
            result.extend(peak.leaves_in_order())
        return result

    def all_merkle_primes(self):
        """Get all Merkle primes across all peaks."""
        result = []
        for peak in self.peaks:
            result.extend(peak.merkle_primes())
        return result

    def peak_heights(self):
        """Return list of peak heights."""
        return [p.height for p in self.peaks]

    @classmethod
    def from_leaves(cls, leaf_primes):
        """Create a PMR by appending specific leaf primes.

        Merkle primes are generated after max(leaf_primes) to guarantee
        disjointness.  This matches merkelize_primes.py's convention and
        makes the result deterministic given the leaf sequence.
        """
        if not leaf_primes:
            return cls()

        max_leaf = max(leaf_primes)
        # We need at most N-1 Merkle primes (for N leaves).
        # Pre-generate them so allocation is deterministic.
        n = len(leaf_primes)
        n_merkle_needed = n - popcount(n)
        merkle_pool = generate_n_primes_after(max_leaf, max(n_merkle_needed, 1))

        pmr = cls.__new__(cls)
        pmr.n_leaves = 0
        pmr.leaf_primes = []
        pmr.peaks = []
        pmr._merkle_pool = merkle_pool
        pmr._merkle_pool_cursor = 0

        for p in leaf_primes:
            pmr._append_with_pool(p)

        return pmr

    def _append_with_pool(self, leaf):
        """Append using pre-generated Merkle pool (for from_leaves)."""
        self.n_leaves += 1
        self.leaf_primes.append(leaf)
        self.peaks.append(PeakNode(leaf, height=0))

        while (len(self.peaks) >= 2 and
               self.peaks[-1].height == self.peaks[-2].height):
            right = self.peaks.pop()
            left = self.peaks.pop()
            m = self._merkle_pool[self._merkle_pool_cursor]
            self._merkle_pool_cursor += 1
            merged = PeakNode(m, left, right, height=left.height + 1)
            self.peaks.append(merged)

    def __repr__(self):
        heights = self.peak_heights()
        return f"PMR(n={self.n_leaves}, peaks={heights})"


# ---------------------------------------------------------------------------
# Parse
# ---------------------------------------------------------------------------

def parse_peak(sequence, cursor, merkle_set, height):
    """Parse a single peak of given height from the sequence.

    Returns (PeakNode, new_cursor).
    """
    if cursor >= len(sequence):
        raise ValueError(f"Unexpected end of sequence at position {cursor}")

    p = sequence[cursor]
    cursor += 1

    if height == 0:
        # Must be a leaf
        if p in merkle_set:
            raise ValueError(f"Expected leaf at position {cursor-1}, got Merkle prime {p}")
        return PeakNode(p, height=0), cursor
    else:
        # Must be an internal node
        if p not in merkle_set:
            raise ValueError(f"Expected Merkle prime at position {cursor-1}, got leaf {p}")
        left, cursor = parse_peak(sequence, cursor, merkle_set, height - 1)
        right, cursor = parse_peak(sequence, cursor, merkle_set, height - 1)
        return PeakNode(p, left, right, height=height), cursor


def parse_pmr(sequence):
    """Parse a serialized PMR sequence.

    The format is self-describing:
    1. L = len(sequence) -> solve N from 2N - popcount(N) = L
    2. Binary decomposition of N gives peak heights
    3. Sort: N smallest are leaves, rest are Merkle
    4. Parse peaks left-to-right using pre-order traversal

    Returns (peaks, leaves_in_order, N) or raises ValueError.
    """
    L = len(sequence)

    if L == 0:
        return [], [], 0

    if L == 1:
        return [PeakNode(sequence[0], height=0)], [sequence[0]], 1

    N = find_N_from_length(L)
    if N is None:
        raise ValueError(f"Invalid sequence length {L}: no N satisfies 2N - popcount(N) = {L}")

    M = N - popcount(N)  # number of Merkle primes

    # Identify leaves vs Merkle by sorting
    sorted_seq = sorted(sequence)

    # Check for duplicates
    for i in range(len(sorted_seq) - 1):
        if sorted_seq[i] == sorted_seq[i + 1]:
            raise ValueError(f"Sequence contains duplicate prime {sorted_seq[i]}")

    merkle_set = set(sorted_seq[N:])

    if len(merkle_set) != M:
        raise ValueError(f"Expected {M} Merkle primes, got {len(merkle_set)}")

    # Peak heights from binary decomposition of N (MSB to LSB)
    peak_heights = []
    for bit in range(N.bit_length() - 1, -1, -1):
        if N & (1 << bit):
            peak_heights.append(bit)

    # Parse peaks
    cursor = 0
    peaks = []
    for h in peak_heights:
        peak, cursor = parse_peak(sequence, cursor, merkle_set, h)
        peaks.append(peak)

    if cursor != L:
        raise ValueError(f"Sequence not fully consumed: parsed {cursor} of {L}")

    # Extract leaves in order
    leaves = []
    for peak in peaks:
        leaves.extend(peak.leaves_in_order())

    return peaks, leaves, N


# ---------------------------------------------------------------------------
# Verify
# ---------------------------------------------------------------------------

def verify_pmr(sequence):
    """Verify a PMR sequence is valid.

    Checks:
    1. Parseable (valid length, valid structure)
    2. Leaves and Merkle primes are disjoint (Merkle > all leaves)
    3. Peak structure matches binary decomposition of N
    4. Round-trip: rebuild from leaves produces same serialization

    Returns (is_valid, leaves, error_message).
    """
    try:
        peaks, leaves, N = parse_pmr(sequence)
    except ValueError as e:
        return False, [], str(e)

    if N == 0:
        return True, [], "Empty PMR"

    # Check disjointness: all Merkle primes > all leaf primes
    all_leaves_set = set()
    all_merkle_set = set()
    for peak in peaks:
        for p in peak.leaves_in_order():
            all_leaves_set.add(p)
        for p in peak.merkle_primes():
            all_merkle_set.add(p)

    if all_leaves_set & all_merkle_set:
        overlap = all_leaves_set & all_merkle_set
        return False, leaves, f"Leaves and Merkle primes overlap: {overlap}"

    if all_merkle_set and all_leaves_set:
        if min(all_merkle_set) <= max(all_leaves_set):
            return False, leaves, (
                f"Merkle primes not all > leaves: min_merkle={min(all_merkle_set)}, "
                f"max_leaf={max(all_leaves_set)}"
            )

    # Round-trip: rebuild PMR from leaves using from_leaves (deterministic)
    pmr = PrimeMountainRange.from_leaves(leaves)
    rebuilt = pmr.serialize()

    if rebuilt != sequence:
        return False, leaves, (
            f"Round-trip mismatch:\n"
            f"  original: {sequence}\n"
            f"  rebuilt:  {rebuilt}"
        )

    return True, leaves, "Valid PMR"


# ---------------------------------------------------------------------------
# Visualization
# ---------------------------------------------------------------------------

def draw_peak(peak, prefix="", is_last=True, is_root=True):
    """Draw a peak tree visualization."""
    MERKLE_COLOR = '\033[92m'
    LEAF_COLOR = '\033[94m'
    RESET_COLOR = '\033[0m'

    value = str(peak.prime)

    if is_root:
        if peak.is_leaf():
            print(f"{LEAF_COLOR}{value}{RESET_COLOR}")
        else:
            print(f"{MERKLE_COLOR}{value}{RESET_COLOR}")
    else:
        connector = "\u2514\u2500\u2500 " if is_last else "\u251c\u2500\u2500 "
        if peak.is_leaf():
            print(f"{prefix}{connector}{LEAF_COLOR}{value}{RESET_COLOR}")
        else:
            print(f"{prefix}{connector}{MERKLE_COLOR}{value}{RESET_COLOR}")

    if not peak.is_leaf():
        child_prefix = prefix + ("    " if is_last or is_root else "\u2502   ")
        if peak.left:
            draw_peak(peak.left, child_prefix if not is_root else "", False, False)
        if peak.right:
            draw_peak(peak.right, child_prefix if not is_root else "", True, False)


# ---------------------------------------------------------------------------
# Tests
# ---------------------------------------------------------------------------

def run_tests():
    """Run all tests."""
    passed = 0
    failed = 0

    def check(name, condition, detail=""):
        nonlocal passed, failed
        if condition:
            print(f"  PASS {name}")
            passed += 1
        else:
            print(f"  FAIL {name}: {detail}")
            failed += 1

    print("Running PMR tests...\n")

    # --- find_N_from_length ---
    print("find_N_from_length:")
    for N in range(0, 33):
        L = 2 * N - popcount(N)
        result = find_N_from_length(L)
        check(f"N={N} -> L={L} -> N={result}", result == N, f"got {result}")

    # invalid lengths (L=2 and L=6 have no valid N)
    check("L=2 -> None", find_N_from_length(2) is None)
    check("L=6 -> None", find_N_from_length(6) is None)

    # --- Basic append (sieve mode) ---
    print("\nBasic append (sieve mode):")
    pmr = PrimeMountainRange()

    pmr.append()
    check("N=1: single peak h=0", len(pmr.peaks) == 1 and pmr.peaks[0].height == 0)

    pmr.append()
    check("N=2: single peak h=1", len(pmr.peaks) == 1 and pmr.peaks[0].height == 1)

    pmr.append()
    check("N=3: peaks [1, 0]", pmr.peak_heights() == [1, 0],
          f"got {pmr.peak_heights()}")

    pmr.append()
    check("N=4: single peak h=2", len(pmr.peaks) == 1 and pmr.peaks[0].height == 2,
          f"got {pmr.peak_heights()}")

    pmr.append()
    check("N=5: peaks [2, 0]", pmr.peak_heights() == [2, 0],
          f"got {pmr.peak_heights()}")

    # --- popcount correspondence ---
    print("\nPeak count == popcount(N):")
    for N in [1, 2, 3, 7, 8, 13, 15, 16, 31, 32]:
        pmr = PrimeMountainRange()
        for _ in range(N):
            pmr.append()
        num_peaks = len(pmr.peaks)
        expected = popcount(N)
        check(f"N={N}: {num_peaks} peaks == popcount({N})={expected}",
              num_peaks == expected, f"got {num_peaks}")

    # --- Merkle prime count ---
    print("\nMerkle count == N - popcount(N):")
    for N in [1, 2, 4, 7, 8, 13, 16]:
        pmr = PrimeMountainRange()
        for _ in range(N):
            pmr.append()
        merkle_count = len(pmr.all_merkle_primes())
        expected = N - popcount(N)
        check(f"N={N}: {merkle_count} == {expected}",
              merkle_count == expected, f"got {merkle_count}")

    # --- Serialization length ---
    print("\nSerialization length == 2N - popcount(N):")
    for N in [1, 2, 3, 4, 7, 8, 13, 16]:
        pmr = PrimeMountainRange()
        for _ in range(N):
            pmr.append()
        seq = pmr.serialize()
        expected_len = 2 * N - popcount(N)
        check(f"N={N}: len={len(seq)} == {expected_len}",
              len(seq) == expected_len, f"got {len(seq)}")

    # --- Disjointness ---
    print("\nLeaf/Merkle disjointness:")
    for N in [2, 5, 8, 13, 16]:
        pmr = PrimeMountainRange()
        for _ in range(N):
            pmr.append()
        leaf_set = set(pmr.all_leaves())
        merkle_set = set(pmr.all_merkle_primes())
        overlap = leaf_set & merkle_set
        check(f"N={N}: disjoint", len(overlap) == 0, f"overlap: {overlap}")
        if merkle_set and leaf_set:
            check(f"N={N}: all Merkle > all leaves",
                  min(merkle_set) > max(leaf_set),
                  f"min_merkle={min(merkle_set)}, max_leaf={max(leaf_set)}")

    # --- Round-trip (sieve mode): serialize -> parse -> check leaves ---
    print("\nRound-trip (sieve mode):")
    for N in [1, 2, 3, 4, 5, 7, 8, 13, 15, 16]:
        pmr = PrimeMountainRange()
        for _ in range(N):
            pmr.append()
        original_leaves = pmr.all_leaves()
        seq = pmr.serialize()

        try:
            peaks, parsed_leaves, parsed_N = parse_pmr(seq)
            check(f"N={N}: parsed N={parsed_N}", parsed_N == N, f"got {parsed_N}")
            check(f"N={N}: leaves match", parsed_leaves == original_leaves,
                  f"original={original_leaves}, parsed={parsed_leaves}")
        except ValueError as e:
            check(f"N={N}: parse succeeded", False, str(e))

    # --- Round-trip (from_leaves): serialize -> parse -> verify ---
    print("\nRound-trip (from_leaves / verify):")
    test_leaf_sets = [
        [2],
        [2, 3],
        [2, 3, 5],
        [2, 3, 5, 7],
        [2, 3, 5, 7, 11],
        [2, 3, 5, 7, 11, 13, 17],
        [2, 3, 5, 7, 11, 13, 17, 19],
        [2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37, 41],
        [2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37, 41, 43, 47, 53],
    ]
    for leaves in test_leaf_sets:
        N = len(leaves)
        pmr = PrimeMountainRange.from_leaves(leaves)
        seq = pmr.serialize()
        is_valid, parsed_leaves, msg = verify_pmr(seq)
        check(f"N={N} from_leaves: valid", is_valid, msg)
        check(f"N={N} from_leaves: leaves match", parsed_leaves == leaves,
              f"expected {leaves}, got {parsed_leaves}")

    # --- Power-of-2: single peak, L = 2N-1 ---
    print("\nPower-of-2 (single peak, L = 2N-1):")
    for k in range(1, 6):
        N = 2 ** k
        pmr = PrimeMountainRange()
        for _ in range(N):
            pmr.append()
        check(f"N={N}: single peak", len(pmr.peaks) == 1)
        seq = pmr.serialize()
        check(f"N={N}: L = {len(seq)} == {2*N-1}", len(seq) == 2 * N - 1)

    # --- Power-of-2 compatibility with merkelize_primes.py ---
    print("\nPower-of-2 compatibility with merkelize_primes.py:")
    for k in range(1, 5):
        N = 2 ** k
        # Build from_leaves with first N primes
        first_n_primes = generate_primes_up_to_inclusive(200)[:N]
        pmr = PrimeMountainRange.from_leaves(first_n_primes)
        seq = pmr.serialize()

        # Compare: merkelize_primes uses generate_n_primes_after(max_leaf, N-1)
        # and builds a balanced tree with BFS Merkle assignment (root = largest).
        # PMR assigns Merkle primes ascending bottom-up, so root also gets largest.
        # For N = power-of-2: single peak, L = 2N-1 (matches old format length).
        check(f"N={N}: L = {len(seq)} == {2*N-1} (matches old format)",
              len(seq) == 2 * N - 1)

        # Verify the Merkle primes are the same set as merkelize_primes would use
        max_leaf = max(first_n_primes)
        expected_merkle = generate_n_primes_after(max_leaf, N - 1)
        actual_merkle = sorted(pmr.all_merkle_primes())
        expected_merkle_sorted = sorted(expected_merkle)
        check(f"N={N}: same Merkle prime set",
              actual_merkle == expected_merkle_sorted,
              f"expected {expected_merkle_sorted}, got {actual_merkle}")

    # --- Edge cases ---
    print("\nEdge cases:")
    # N=0
    pmr = PrimeMountainRange()
    check("N=0: no peaks", len(pmr.peaks) == 0)
    check("N=0: serialize empty", pmr.serialize() == [])

    # N=1 parse
    try:
        peaks, leaves, n = parse_pmr([2])
        check("Parse [2]: N=1, leaf=2", n == 1 and leaves == [2])
    except ValueError as e:
        check("Parse [2]", False, str(e))

    # Invalid lengths
    check("L=2 invalid", find_N_from_length(2) is None)
    try:
        parse_pmr([2, 3])
        check("Parse [2,3]: raises ValueError", False, "should have raised")
    except ValueError:
        check("Parse [2,3]: raises ValueError", True)

    # Ascending Merkle within peaks (bottom-up allocation)
    print("\nAscending Merkle within peaks:")
    for N in [4, 8, 16]:
        pmr = PrimeMountainRange.from_leaves(generate_primes_up_to_inclusive(200)[:N])
        for peak in pmr.peaks:
            merkle_list = peak.merkle_primes()
            if len(merkle_list) > 1:
                # Root should have largest Merkle prime
                check(f"N={N}: root has largest Merkle",
                      merkle_list[0] == max(merkle_list),
                      f"root={merkle_list[0]}, max={max(merkle_list)}")

    # Summary
    print(f"\n{'='*60}")
    print(f"Results: {passed} passed, {failed} failed")
    return failed == 0


# ---------------------------------------------------------------------------
# Comparison
# ---------------------------------------------------------------------------

def run_comparison():
    """Compare PMR prime-finding with traditional approaches."""
    sys.path.insert(0, os.path.dirname(__file__))
    from merkelize_primes import merkleize_primes

    BOLD = '\033[1m'
    RESET = '\033[0m'

    def fmt_time(t):
        """Format a time duration in human-readable form."""
        if t < 1e-6:
            return f"{t*1e9:.0f} ns"
        elif t < 1e-3:
            return f"{t*1e6:.1f} us"
        elif t < 1:
            return f"{t*1e3:.2f} ms"
        else:
            return f"{t:.3f} s"

    def print_table(headers, rows):
        """Print a formatted table."""
        widths = [max(len(str(h)), max((len(str(r[i])) for r in rows), default=0)) + 2
                  for i, h in enumerate(headers)]
        print(f"  {''.join(str(h).ljust(w) for h, w in zip(headers, widths))}")
        print(f"  {''.join('-' * w for w in widths)}")
        for row in rows:
            print(f"  {''.join(str(c).ljust(w) for c, w in zip(row, widths))}")

    def first_n_primes(n):
        """Get first n primes via sieve."""
        bound = max(n * 15, 100)
        primes = sieve_primes(bound)
        while len(primes) < n:
            bound *= 2
            primes = sieve_primes(bound)
        return primes[:n]

    # -------------------------------------------------------------------
    # Comparison 1: Extract next prime from parsed PMR vs trial division
    # -------------------------------------------------------------------
    print(f"\n{BOLD}Comparison 1: Extract p_{{N+1}} from parsed PMR vs trial division{RESET}")
    print("  PMR: sort serialized sequence, take element at index N")
    print("  Traditional: trial-divide candidates after p_N\n")

    rows = []
    for N in [100, 1000, 10000]:
        leaves = first_n_primes(N)
        pmr = PrimeMountainRange.from_leaves(leaves)
        seq = pmr.serialize()

        iters = max(1, 10000 // N)

        t0 = time.perf_counter()
        for _ in range(iters):
            next_pmr = sorted(seq)[N]
        t1 = time.perf_counter()
        time_pmr = (t1 - t0) / iters

        p_N = leaves[-1]
        t0 = time.perf_counter()
        for _ in range(iters):
            next_td = generate_n_primes_after(p_N, 1)[0]
        t1 = time.perf_counter()
        time_td = (t1 - t0) / iters

        assert next_pmr == next_td, f"N={N}: PMR={next_pmr} != TD={next_td}"

        speedup = time_td / time_pmr if time_pmr > 0 else float('inf')
        rows.append([N, f"p_{N+1}={next_pmr}", fmt_time(time_pmr),
                     fmt_time(time_td), f"{speedup:.1f}x"])

    print_table(["N", "Result", "PMR (sort)", "Trial div", "Speedup"], rows)

    # -------------------------------------------------------------------
    # Comparison 2: Batch prime generation (sieve vs trial division)
    # -------------------------------------------------------------------
    print(f"\n{BOLD}Comparison 2: Batch prime generation (sieve vs trial division){RESET}")
    print("  PMR sieve: Sieve of Eratosthenes up to bound")
    print("  Traditional: trial division for each candidate\n")

    rows = []
    for N in [1000, 10000, 100000]:
        bound = int(N * (math.log(N) + math.log(math.log(N)))) + 100

        # Sieve
        t0 = time.perf_counter()
        sieve_result = sieve_primes(bound)
        t1 = time.perf_counter()
        while len(sieve_result) < N:
            bound *= 2
            t0 = time.perf_counter()
            sieve_result = sieve_primes(bound)
            t1 = time.perf_counter()
        time_sieve = t1 - t0

        # Trial division up to same bound
        t0 = time.perf_counter()
        td_result = generate_primes_up_to_inclusive(bound)
        t1 = time.perf_counter()
        time_td = t1 - t0

        speedup = time_td / time_sieve if time_sieve > 0 else float('inf')
        rows.append([N, bound, fmt_time(time_sieve), fmt_time(time_td),
                     f"{speedup:.1f}x"])

    print_table(["N", "Bound", "Sieve", "Trial div", "Speedup"], rows)

    # -------------------------------------------------------------------
    # Comparison 3: Incremental append (PMR vs full Merkle rebuild)
    # -------------------------------------------------------------------
    print(f"\n{BOLD}Comparison 3: Incremental append (PMR vs full Merkle rebuild){RESET}")
    print("  PMR: append() x N (O(1) amortized cursor advance)")
    print("  Traditional: merkleize_primes(leaves[:k]) for k = 1..N\n")

    rows = []
    for N in [100, 500, 1000]:
        leaves = first_n_primes(N)

        # PMR incremental append
        t0 = time.perf_counter()
        pmr = PrimeMountainRange()
        for p in leaves:
            pmr.append(p)
        t1 = time.perf_counter()
        time_pmr = t1 - t0

        # Full rebuild at each step
        t0 = time.perf_counter()
        for k in range(1, N + 1):
            merkleize_primes(leaves[:k])
        t1 = time.perf_counter()
        time_rebuild = t1 - t0

        speedup = time_rebuild / time_pmr if time_pmr > 0 else float('inf')
        rows.append([N, fmt_time(time_pmr), fmt_time(time_rebuild),
                     f"{speedup:.1f}x"])

    print_table(["N", "PMR append", "Full rebuild x N", "Speedup"], rows)

    print()


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

def main():
    parser = argparse.ArgumentParser(
        description='Prime Mountain Range -- append-optimized Merkle structure for primes',
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  prime_mountain_range.py "2,3,5,7,11"          # Append 5 primes
  prime_mountain_range.py -N 16                  # Auto-generate 16 leaf primes
  prime_mountain_range.py -N 13 -v               # Verbose: show peak trees
  prime_mountain_range.py -p "..."               # Parse serialized sequence
  prime_mountain_range.py --verify "..."         # Verify round-trip
  prime_mountain_range.py --bench 10000          # Benchmark append throughput
  prime_mountain_range.py --test                 # Run all tests
        """
    )
    parser.add_argument('primes', type=str, nargs='?',
                        help='Primes to append (comma-separated)')
    parser.add_argument('-N', '--num', type=int, metavar='N',
                        help='Auto-generate N leaf primes from sieve')
    parser.add_argument('--parse', '-p', type=str, nargs='?', const='',
                        help='Parse a serialized PMR sequence')
    parser.add_argument('--verify', type=str, nargs='?', const='',
                        help='Verify a serialized PMR sequence')
    parser.add_argument('--bench', type=int, metavar='N',
                        help='Benchmark: append N leaves and report throughput')
    parser.add_argument('--test', action='store_true',
                        help='Run all tests')
    parser.add_argument('--verbose', '-v', action='store_true',
                        help='Show detailed output with peak trees')
    parser.add_argument('--compare', action='store_true',
                        help='Compare with merkelize_primes.py for power-of-2 cases')

    args = parser.parse_args()

    MERKLE_COLOR = '\033[92m'
    LEAF_COLOR = '\033[94m'
    RESET_COLOR = '\033[0m'

    # --test
    if args.test:
        success = run_tests()
        sys.exit(0 if success else 1)

    # --bench
    if args.bench:
        N = args.bench
        print(f"Benchmarking PMR with {N} appends...")

        # Generate first N primes as leaves
        bound = max(N * 15, 100)
        all_primes = sieve_primes(bound)
        while len(all_primes) < N:
            bound *= 2
            all_primes = sieve_primes(bound)
        leaf_primes = all_primes[:N]

        t0 = time.time()
        pmr = PrimeMountainRange.from_leaves(leaf_primes)
        t1 = time.time()

        elapsed = t1 - t0
        rate = N / elapsed if elapsed > 0 else float('inf')

        print(f"  Built {N}-leaf PMR in {elapsed:.4f}s ({rate:.0f} leaves/sec)")
        print(f"  Peaks: {len(pmr.peaks)} (popcount({N}) = {popcount(N)})")
        print(f"  Merkle primes used: {len(pmr.all_merkle_primes())}")

        # Serialization benchmark
        t0 = time.time()
        seq = pmr.serialize()
        t1 = time.time()
        print(f"  Serialized {len(seq)} elements in {t1-t0:.4f}s")

        # Parse benchmark
        t0 = time.time()
        peaks, leaves, parsed_N = parse_pmr(seq)
        t1 = time.time()
        print(f"  Parsed in {t1-t0:.4f}s (N={parsed_N})")

        sys.exit(0)

    # --compare
    if args.compare:
        run_comparison()
        sys.exit(0)

    # --parse
    if args.parse is not None:
        parse_input = args.parse if args.parse else None
        if not parse_input and not sys.stdin.isatty():
            parse_input = sys.stdin.read().strip()
        if not parse_input:
            print("Error: --parse requires input", file=sys.stderr)
            sys.exit(1)

        try:
            sequence = [int(x.strip()) for x in parse_input.split(',')]
        except ValueError as e:
            print(f"Error: {e}", file=sys.stderr)
            sys.exit(1)

        try:
            peaks, leaves, N = parse_pmr(sequence)
        except ValueError as e:
            print(f"Error: {e}", file=sys.stderr)
            sys.exit(1)

        if not args.verbose:
            print(','.join(str(p) for p in leaves))
            sys.exit(0)

        print(f"\nParsed PMR (N={N}, {len(peaks)} peaks):")
        print("=" * 60)
        print(f"Leaves: {LEAF_COLOR}{leaves}{RESET_COLOR}")
        print(f"Peak heights: {[p.height for p in peaks]}")
        for i, peak in enumerate(peaks):
            print(f"\nPeak {i} (height {peak.height}):")
            draw_peak(peak)
        print("=" * 60)
        sys.exit(0)

    # --verify
    if args.verify is not None:
        verify_input = args.verify if args.verify else None
        if not verify_input and not sys.stdin.isatty():
            verify_input = sys.stdin.read().strip()
        if not verify_input:
            print("Error: --verify requires input", file=sys.stderr)
            sys.exit(1)

        try:
            sequence = [int(x.strip()) for x in verify_input.split(',')]
        except ValueError as e:
            print(f"Error: {e}", file=sys.stderr)
            sys.exit(1)

        is_valid, leaves, msg = verify_pmr(sequence)

        if not args.verbose:
            if is_valid:
                print("valid")
            else:
                print(f"invalid: {msg}", file=sys.stderr)
                sys.exit(1)
            sys.exit(0)

        print(f"\nVerification (N={len(leaves)}):")
        print("=" * 60)
        print(f"Leaves: {LEAF_COLOR}{leaves}{RESET_COLOR}")
        if is_valid:
            print(f"{MERKLE_COLOR}+ {msg}{RESET_COLOR}")
        else:
            print(f"\033[91m- {msg}{RESET_COLOR}")
            sys.exit(1)
        print("=" * 60)
        sys.exit(0)

    # Build PMR from input
    if args.num is not None:
        if args.num < 0:
            print("Error: N must be >= 0", file=sys.stderr)
            sys.exit(1)
        if args.num == 0:
            pmr = PrimeMountainRange()
        else:
            # Generate first N primes, then use from_leaves for deterministic
            # Merkle allocation (compatible with verify round-trip).
            bound = max(args.num * 15, 100)
            all_primes = sieve_primes(bound)
            while len(all_primes) < args.num:
                bound *= 2
                all_primes = sieve_primes(bound)
            first_n = all_primes[:args.num]
            pmr = PrimeMountainRange.from_leaves(first_n)
    elif args.primes:
        try:
            input_primes = [int(x.strip()) for x in args.primes.split(',')]
        except ValueError as e:
            print(f"Error: {e}", file=sys.stderr)
            sys.exit(1)

        for p in input_primes:
            if not is_prime(p):
                print(f"Error: {p} is not prime", file=sys.stderr)
                sys.exit(1)

        pmr = PrimeMountainRange.from_leaves(input_primes)
    elif not sys.stdin.isatty():
        try:
            stdin_data = sys.stdin.read().strip()
            if stdin_data:
                input_primes = [int(x.strip()) for x in stdin_data.split(',')]
                for p in input_primes:
                    if not is_prime(p):
                        print(f"Error: {p} is not prime", file=sys.stderr)
                        sys.exit(1)
                pmr = PrimeMountainRange.from_leaves(input_primes)
            else:
                parser.print_help()
                sys.exit(1)
        except ValueError as e:
            print(f"Error: {e}", file=sys.stderr)
            sys.exit(1)
    else:
        parser.print_help()
        sys.exit(1)

    # Output
    seq = pmr.serialize()
    merkle_set = set(pmr.all_merkle_primes())

    if not args.verbose:
        # Quiet: colored sequence
        parts = []
        for p in seq:
            if p in merkle_set:
                parts.append(f"{MERKLE_COLOR}{p}{RESET_COLOR}")
            else:
                parts.append(str(p))
        print(','.join(parts))
        sys.exit(0)

    # Verbose output
    leaves = pmr.all_leaves()
    print(f"\nPrime Mountain Range (N={pmr.n_leaves})")
    print("=" * 60)
    print(f"Leaves ({len(leaves)}): {LEAF_COLOR}{leaves}{RESET_COLOR}")
    print(f"Merkle primes ({len(merkle_set)}): {MERKLE_COLOR}{sorted(merkle_set)}{RESET_COLOR}")
    print(f"Peaks: {len(pmr.peaks)} (heights: {pmr.peak_heights()})")

    if hasattr(pmr, 'peek_next_leaf'):
        try:
            print(f"Next leaf prime: {LEAF_COLOR}{pmr.peek_next_leaf()}{RESET_COLOR}")
        except (AttributeError, IndexError):
            pass

    for i, peak in enumerate(pmr.peaks):
        print(f"\nPeak {i} (height {peak.height}):")
        draw_peak(peak)

    # Serialized sequence
    print(f"\nSerialized ({len(seq)} elements):")
    print("=" * 60)
    parts = []
    for p in seq:
        if p in merkle_set:
            parts.append(f"{MERKLE_COLOR}{p}{RESET_COLOR}")
        else:
            parts.append(f"{LEAF_COLOR}{p}{RESET_COLOR}")
    print(','.join(parts))
    print("=" * 60)
    print(f"\nPlain: {','.join(str(p) for p in seq)}")

    # Self-verify
    is_valid, _, msg = verify_pmr(seq)
    if is_valid:
        print(f"\n{MERKLE_COLOR}+ Self-verified{RESET_COLOR}")
    else:
        print(f"\n\033[91m- Self-verification FAILED: {msg}{RESET_COLOR}", file=sys.stderr)


if __name__ == "__main__":
    main()
