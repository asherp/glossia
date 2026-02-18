#!/usr/bin/env python3
"""
Exact Harmonic Oscillator with PMR Entropy Tracking.

Simulates H = p^2/2m + kx^2/2 using symplectic (Stormer-Verlet) leapfrog
integration in both exact rational arithmetic (Python Fraction) and float64.

The leapfrog integrator is symplectic: it conserves a shadow Hamiltonian
exactly, so |dE| is bounded (O(dt^2)) with no secular drift.
Float64 silently injects noise, causing energy drift beyond that bound.

The cost of exactness is representation growth: numerators and denominators
grow exponentially (~O(10^N) bits after N steps). The Prime Mountain Range
(PMR) tracks this growth as a computable entropy measure by recording which
new prime factors appear in the state at each timestep.

Usage:
  python harmonic_oscillator.py                    # 100 steps, dt=1/10
  python harmonic_oscillator.py --steps 500        # more steps
  python harmonic_oscillator.py --dt 1/4           # larger timestep
  python harmonic_oscillator.py --steps 50 --dt 1/4 --x0 3/2 --p0 1/2
"""

import sys
import os
import argparse
import math
from fractions import Fraction

# Import PMR from sibling
sys.path.insert(0, os.path.dirname(__file__))
from prime_mountain_range import PrimeMountainRange


# ---------------------------------------------------------------------------
# Fast factorization (Pollard's rho + Miller-Rabin)
# ---------------------------------------------------------------------------

def _isqrt(n):
    """Integer square root (floor). Works for arbitrarily large n."""
    if n < 0:
        raise ValueError("Square root not defined for negative numbers")
    if n < 2:
        return n
    # Newton's method
    x = n
    y = (x + 1) // 2
    while y < x:
        x = y
        y = (x + n // x) // 2
    return x


def _miller_rabin(n, witnesses=(2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37)):
    """Deterministic Miller-Rabin for n < 3.3e24; probabilistic beyond."""
    if n < 2:
        return False
    if n < 4:
        return True
    if n % 2 == 0:
        return False
    d, r = n - 1, 0
    while d % 2 == 0:
        d //= 2
        r += 1
    for a in witnesses:
        if a >= n:
            continue
        x = pow(a, d, n)
        if x == 1 or x == n - 1:
            continue
        for _ in range(r - 1):
            x = pow(x, 2, n)
            if x == n - 1:
                break
        else:
            return False
    return True


def _pollard_rho(n):
    """Find a non-trivial factor of n using Pollard's rho with Brent's cycle detection."""
    if n % 2 == 0:
        return 2
    per_c_budget = 50000
    for c in range(1, 200):
        y = 2
        d = 1
        q, x, ys = 1, y, y
        iters = 0
        while d == 1 and iters < per_c_budget:
            x = y
            for _ in range(128):
                y = (y * y + c) % n
                q = q * abs(x - y) % n
                iters += 1
            d = math.gcd(q, n)
            if d == 1:
                ys = y
        if d == n:
            # GCD accumulated to n; step one at a time
            d = 1
            for _ in range(10000):
                ys = (ys * ys + c) % n
                d = math.gcd(abs(x - ys), n)
                if d != 1:
                    break
        if 1 < d < n:
            return d
    return n


_POLLARD_DIGIT_LIMIT = 30  # only attempt Pollard's rho on numbers < 10^30

def fast_factor(n):
    """Return the set of prime factors of n using fast methods.

    Uses trial division for small factors, Miller-Rabin for primality,
    and Pollard's rho for composites up to ~30 digits.
    Larger composites are treated as opaque (returned as-is).
    """
    n = abs(n)
    if n < 2:
        return set()
    factors = set()
    # Trial division for small primes
    for p in (2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37, 41, 43, 47):
        if p * p > n:
            break
        if n % p == 0:
            factors.add(p)
            while n % p == 0:
                n //= p
    if n < 2:
        return factors
    # Extended trial division up to ~1000
    for p in range(53, min(1000, _isqrt(n) + 1), 2):
        if n % p == 0:
            factors.add(p)
            while n % p == 0:
                n //= p
    if n < 2:
        return factors
    # Recursive factorization of remaining composite
    stack = [n]
    while stack:
        m = stack.pop()
        if m < 2:
            continue
        if _miller_rabin(m):
            factors.add(m)
            continue
        # Only attempt Pollard's rho on manageable composites
        if len(str(m)) > _POLLARD_DIGIT_LIMIT:
            factors.add(m)  # opaque composite, treat as single unit
            continue
        d = _pollard_rho(m)
        if d == m:
            factors.add(m)  # couldn't factor further
        else:
            stack.append(d)
            stack.append(m // d)
    return factors


# ---------------------------------------------------------------------------
# Prime tracking
# ---------------------------------------------------------------------------

def find_new_primes(n, known_primes):
    """Find prime factors of n not already in known_primes.

    Divides out known primes first so the remainder to factorize is small.
    """
    remainder = abs(n)
    if remainder < 2:
        return set()
    for p in known_primes:
        while remainder % p == 0:
            remainder //= p
    if remainder <= 1:
        return set()
    return fast_factor(remainder)


def state_new_primes(x, p, known_primes):
    """Find all new primes in a Fraction state (x, p).

    Checks numerators and denominators of both x and p.
    """
    new = set()
    for val in (x.numerator, x.denominator, p.numerator, p.denominator):
        new |= find_new_primes(val, known_primes)
    return new


# ---------------------------------------------------------------------------
# Physics
# ---------------------------------------------------------------------------

def energy(x, p, k, m):
    """Hamiltonian: H = p^2/(2m) + k*x^2/2."""
    return p * p / (2 * m) + k * x * x / 2


def leapfrog_step_exact(x, p, k, m, dt):
    """One Stormer-Verlet leapfrog step with Fraction arithmetic."""
    half_dt = dt / 2
    p_half = p - k * x * half_dt
    x_new = x + p_half * dt / m
    p_new = p_half - k * x_new * half_dt
    return x_new, p_new


def leapfrog_step_float(x, p, k, m, dt):
    """One Stormer-Verlet leapfrog step with float64 arithmetic."""
    half_dt = dt / 2.0
    p_half = p - k * x * half_dt
    x_new = x + p_half * dt / m
    p_new = p_half - k * x_new * half_dt
    return x_new, p_new


# ---------------------------------------------------------------------------
# Simulation
# ---------------------------------------------------------------------------

def run_simulation(steps, dt, k, m, x0, p0):
    """Run the exact and float64 simulations, tracking prime basis growth."""

    # Exact state (Fraction)
    dt_f = Fraction(dt)
    k_f = Fraction(k)
    m_f = Fraction(m)
    x_exact = Fraction(x0)
    p_exact = Fraction(p0)
    E0_exact = energy(x_exact, p_exact, k_f, m_f)

    # Float state
    x_float = float(x0)
    p_float = float(p0)
    k_fl = float(k)
    m_fl = float(m)
    dt_fl = float(dt)
    E0_float = energy(x_float, p_float, k_fl, m_fl)

    # Prime tracking
    known_primes = set()
    # Seed with primes from initial state and parameters
    for val in (x_exact.numerator, x_exact.denominator,
                p_exact.numerator, p_exact.denominator,
                k_f.numerator, k_f.denominator,
                m_f.numerator, m_f.denominator,
                dt_f.numerator, dt_f.denominator):
        known_primes |= fast_factor(val)

    pmr = PrimeMountainRange(sieve_bound=10000)

    # Append initial primes as leaves
    initial_primes = sorted(known_primes)
    for pr in initial_primes:
        pmr.append(pr)

    # Records
    # (step, dE_exact, E_float, dE_float) where dE = E - E0
    energy_samples = []
    prime_events = []         # (step, cumulative_basis_size, new_primes)
    sample_interval = max(1, steps // 20)
    max_dE_exact = Fraction(0)  # track max |dE| for exact (bounded oscillation)

    # Record step 0
    energy_samples.append((0, Fraction(0), E0_float, 0.0))
    if initial_primes:
        prime_events.append((0, len(known_primes), sorted(known_primes)))

    # Main loop
    for step in range(1, steps + 1):
        # Exact step
        x_exact, p_exact = leapfrog_step_exact(x_exact, p_exact, k_f, m_f, dt_f)
        E_exact = energy(x_exact, p_exact, k_f, m_f)
        dE_exact = E_exact - E0_exact

        # Float step
        x_float, p_float = leapfrog_step_float(x_float, p_float, k_fl, m_fl, dt_fl)
        E_float = energy(x_float, p_float, k_fl, m_fl)

        if abs(dE_exact) > max_dE_exact:
            max_dE_exact = abs(dE_exact)

        # Sample energy
        if step % sample_interval == 0 or step == steps:
            dE_float = E_float - E0_float
            energy_samples.append((step, dE_exact, E_float, dE_float))

        # Track new primes
        new_primes = state_new_primes(x_exact, p_exact, known_primes)
        if new_primes:
            known_primes |= new_primes
            for pr in sorted(new_primes):
                pmr.append(pr)
            prime_events.append((step, len(known_primes), sorted(new_primes)))

    return (energy_samples, prime_events, pmr,
            x_exact, p_exact, x_float, p_float, E0_exact, max_dE_exact)


# ---------------------------------------------------------------------------
# Output
# ---------------------------------------------------------------------------

def format_fraction_brief(f):
    """Format a Fraction compactly."""
    if f.denominator == 1:
        return str(f.numerator)
    return f"{f.numerator}/{f.denominator}"


def print_results(energy_samples, prime_events, pmr,
                  x_exact, p_exact, x_float, p_float,
                  E0_exact, max_dE_exact, steps):
    """Print the four output phases."""

    BOLD = '\033[1m'
    GREEN = '\033[92m'
    YELLOW = '\033[93m'
    CYAN = '\033[96m'
    RESET = '\033[0m'

    # --- Phase 1: Energy behavior ---
    print(f"\n{BOLD}Phase 1: Energy Behavior{RESET}")
    print(f"  E0 (exact) = {format_fraction_brief(E0_exact)} = {float(E0_exact):.15g}")
    print(f"  Integrator: Stormer-Verlet leapfrog (symplectic)")
    print(f"  Symplectic integrators conserve a shadow Hamiltonian, not H itself.")
    print(f"  Exact |dE| is bounded; float64 |dE| drifts over time.")
    print()

    # Header
    print(f"  {'Step':>6}  {'dE_exact':>16}  {'E_float':>20}  {'dE_float':>14}")
    print(f"  {'─'*6}  {'─'*16}  {'─'*20}  {'─'*14}")

    for step, dE_ex, E_fl, dE_fl in energy_samples:
        dE_ex_float = float(dE_ex)
        print(f"  {step:>6}  {dE_ex_float:>16.6e}  {E_fl:>20.15g}  {dE_fl:>14.6e}")

    print(f"\n  Max |dE_exact|: {float(max_dE_exact):.6e} (bounded)")
    final_dE_float = energy_samples[-1][3]
    print(f"  Final dE_float: {final_dE_float:.6e} (drifting)")
    if abs(final_dE_float) > float(max_dE_exact) * 10:
        print(f"  {YELLOW}Float64 drift exceeds exact bound by "
              f"{abs(final_dE_float) / float(max_dE_exact):.0f}x{RESET}")
    print(f"  {GREEN}Exact computation: no information loss, bounded oscillation.{RESET}")

    # --- Phase 2: Prime basis growth ---
    print(f"\n{BOLD}Phase 2: Prime Basis Growth{RESET}")
    print()
    print(f"  {'Step':>6}  {'Basis size':>10}  New primes")
    print(f"  {'─'*6}  {'─'*10}  {'─'*40}")

    for step, basis_size, new_primes in prime_events:
        primes_str = ', '.join(str(p) for p in new_primes)
        if len(primes_str) > 60:
            primes_str = primes_str[:57] + '...'
        print(f"  {step:>6}  {basis_size:>10}  {primes_str}")

    print(f"\n  Total distinct primes in basis: {CYAN}{prime_events[-1][1]}{RESET}")

    # --- Phase 3: PMR structure ---
    print(f"\n{BOLD}Phase 3: PMR Structure{RESET}")
    heights = pmr.peak_heights()
    print(f"  Leaves (prime basis): {pmr.n_leaves}")
    print(f"  Peak count: {len(pmr.peaks)}")
    print(f"  Peak heights: {heights}")
    seq = pmr.serialize()
    print(f"  Serialization length: {len(seq)}")

    # --- Phase 4: Representation cost ---
    print(f"\n{BOLD}Phase 4: Representation Cost{RESET}")

    x_num_bits = x_exact.numerator.bit_length()
    x_den_bits = x_exact.denominator.bit_length()
    p_num_bits = p_exact.numerator.bit_length()
    p_den_bits = p_exact.denominator.bit_length()
    total_exact_bits = x_num_bits + x_den_bits + p_num_bits + p_den_bits
    float_bits = 128  # two float64s

    print(f"  x numerator:   {x_num_bits:>8} bits")
    print(f"  x denominator: {x_den_bits:>8} bits")
    print(f"  p numerator:   {p_num_bits:>8} bits")
    print(f"  p denominator: {p_den_bits:>8} bits")
    print(f"  {'─'*30}")
    print(f"  Total exact:   {total_exact_bits:>8} bits")
    print(f"  Float64 (x,p): {float_bits:>8} bits")
    print(f"  Ratio:         {total_exact_bits / float_bits:>8.1f}x")

    growth_per_step = total_exact_bits / steps if steps > 0 else 0
    print(f"  Growth rate:   ~{growth_per_step:.1f} bits/step")
    print()


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

def parse_fraction(s):
    """Parse a string like '1/10' or '3' into a Fraction."""
    try:
        return Fraction(s)
    except (ValueError, ZeroDivisionError) as e:
        raise argparse.ArgumentTypeError(f"Invalid fraction '{s}': {e}")


def main():
    parser = argparse.ArgumentParser(
        description='Exact Harmonic Oscillator with PMR Entropy Tracking',
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  python harmonic_oscillator.py                         # 100 steps, dt=1/10
  python harmonic_oscillator.py --steps 500             # more steps
  python harmonic_oscillator.py --dt 1/4                # larger timestep
  python harmonic_oscillator.py --dt 1/4 --steps 50    # custom
  python harmonic_oscillator.py --x0 3/2 --p0 1/2      # custom initial conditions
        """
    )
    parser.add_argument('--steps', type=int, default=100,
                        help='Number of integration steps (default: 100)')
    parser.add_argument('--dt', type=parse_fraction, default=Fraction(1, 10),
                        help='Timestep as fraction string (default: 1/10)')
    parser.add_argument('--k', type=parse_fraction, default=Fraction(1),
                        help='Spring constant (default: 1)')
    parser.add_argument('--m', type=parse_fraction, default=Fraction(1),
                        help='Mass (default: 1)')
    parser.add_argument('--x0', type=parse_fraction, default=Fraction(1),
                        help='Initial position (default: 1)')
    parser.add_argument('--p0', type=parse_fraction, default=Fraction(0),
                        help='Initial momentum (default: 0)')

    args = parser.parse_args()

    if args.steps <= 0:
        print("Error: --steps must be positive", file=sys.stderr)
        sys.exit(1)
    if args.dt <= 0:
        print("Error: --dt must be positive", file=sys.stderr)
        sys.exit(1)
    if args.m <= 0:
        print("Error: --m must be positive", file=sys.stderr)
        sys.exit(1)
    if args.k <= 0:
        print("Error: --k must be positive", file=sys.stderr)
        sys.exit(1)

    print(f"Harmonic Oscillator: H = p^2/(2m) + k*x^2/2")
    print(f"  k={format_fraction_brief(args.k)}, m={format_fraction_brief(args.m)}, "
          f"dt={format_fraction_brief(args.dt)}, "
          f"x0={format_fraction_brief(args.x0)}, p0={format_fraction_brief(args.p0)}")
    print(f"  Steps: {args.steps}")

    results = run_simulation(
        steps=args.steps,
        dt=args.dt,
        k=args.k,
        m=args.m,
        x0=args.x0,
        p0=args.p0,
    )

    (energy_samples, prime_events, pmr,
     x_exact, p_exact, x_float, p_float, E0_exact, max_dE_exact) = results

    print_results(energy_samples, prime_events, pmr,
                  x_exact, p_exact, x_float, p_float,
                  E0_exact, max_dE_exact, args.steps)


if __name__ == '__main__':
    main()
