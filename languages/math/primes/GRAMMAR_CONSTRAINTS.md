# Grammar-Based Constraints

## Overview

The prime ordering constraint can now be **declaratively specified in `grammar.yaml`** instead of being hardcoded in the Rust implementation. This makes the constraint language-specific and easier to maintain.

## Grammar Format

Add a `constraints` section to your `grammar.yaml`:

```yaml
grammar:
  name: "Merkle Tree Grammar"
  
  constraints:
    prime_ordering:
      enabled: true
      description: "Cover words must be non-prime integers satisfying left_prime < cover_word < right_prime"
  
  types:
    # ... type definitions
  rules:
    # ... grammar rules
```

## How It Works

1. **Grammar Loading**: When Glossia loads `grammar.yaml`, it reads the `constraints` section
2. **Constraint Check**: In `generate_text`, the system checks if `prime_ordering.enabled` is `true`
3. **Application**: If enabled, `fill_slots` applies the prime ordering constraint when selecting cover words
4. **Fallback**: If constraint can't be applied (no adjacent primes), falls back to regular cover word selection

## Benefits

- **Declarative**: Constraint is specified in YAML, not hardcoded
- **Language-Specific**: Each language can have its own constraints
- **Maintainable**: Easy to enable/disable or modify without code changes
- **Extensible**: Can add more constraint types in the future

## Example

With `prime_ordering.enabled: true` in `grammar.yaml`:

**Input**: Primes `[3, 5]` with pattern `N Det Det Dot`

**Output**: `"root 3 4 5."` where `4` satisfies `3 < 4 < 5`

## Disabling the Constraint

To disable the constraint, simply set:

```yaml
constraints:
  prime_ordering:
    enabled: false
```

Or omit the `constraints` section entirely (defaults to disabled).

## Implementation Details

The constraint is checked in `generate_text()`:
```rust
let prime_constraint_enabled = grammar.language_config.as_ref()
    .and_then(|config| config.constraints.as_ref())
    .and_then(|constraints| constraints.prime_ordering.as_ref())
    .map(|c| c.enabled)
    .unwrap_or(false);
```

This flag is then passed to `fill_slots()` which applies the constraint when selecting cover words.
