# Prime Ordering Constraint for Cover Words

## Overview

In the `math/primes` language, cover words (non-primes) are constrained to be placed between primes with the ordering rule:

**left_prime < cover_word < right_prime**

## Example

If you have primes `[3, 5]`, a valid sequence would be:
```
3, 4, 5
```

Where:
- `3` = left prime (payload word)
- `4` = cover word (non-prime, satisfies: 3 < 4 < 5)
- `5` = right prime (payload word)

## Implementation

The constraint is implemented in `glossia.rs`:

1. **`pick_cover_with_prime_constraint`** method in `Lexicon`:
   - Filters cover words to only non-prime integers
   - Enforces: `left_bound < cover_word < right_bound`
   - Falls back to regular `pick_cover` if no valid cover word found

2. **`fill_slots`** function:
   - Detects when language is `"math/primes"`
   - Checks if adjacent words are primes
   - Applies the constraint when both left and right primes are found
   - Falls back to regular cover word selection if constraint can't be applied

## How It Works

### Step 1: Detect Adjacent Primes

When filling a slot with a cover word, the system checks:
- **Left word**: Last word in the output (if it's a prime)
- **Right word**: Next payload word (if it's a prime and fits the next slot)

### Step 2: Apply Constraint

If both left and right primes are found:
- Filter cover words to non-prime integers
- Further filter to: `left_prime < cover_word < right_prime`
- Select shortest valid cover word

### Step 3: Fallback

If constraint can't be applied (no adjacent primes):
- Use regular `pick_cover` method
- No ordering constraint enforced

## Cover Word Requirements

For the constraint to work, your `cover.yaml` should include non-prime integers:

```yaml
4:
  N: 1.0
  V: 1.0
  Det: 1.0

6:
  N: 1.0
  V: 1.0

8:
  N: 1.0
  V: 1.0

9:
  N: 1.0
  V: 1.0

10:
  N: 1.0
  V: 1.0

# ... etc (all composite numbers)
```

## Example Workflow

**Input**: Primes `[3, 5]` with grammar pattern `N Det Det Dot`

1. **Slot 0 (N)**: Cover word "root" (no constraint, no adjacent primes)
2. **Slot 1 (Det)**: Prime `3` (payload word)
3. **Slot 2 (Det)**: Cover word selection
   - Left prime: `3`
   - Right prime: `5` (next payload word)
   - Constraint: `3 < cover_word < 5`
   - Valid cover words: `4` (only option)
   - Selected: `4`
4. **Slot 3 (Det)**: Prime `5` (payload word)
5. **Slot 4 (Dot)**: "."

**Output**: `"root 3 4 5."`

## Notes

- The constraint only applies when **both** adjacent words are primes
- Cover words must be **non-prime integers** to satisfy the constraint
- If no valid cover word exists in the range, falls back to regular selection
- The constraint preserves the ordering property needed for merkelization
