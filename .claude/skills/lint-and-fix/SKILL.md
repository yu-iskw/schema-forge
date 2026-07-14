---
name: lint-and-fix
description: Run linters and fix violations, formatting errors, or style mismatches using Trunk. Use when code quality checks fail, before submitting PRs, or to repair "broken" linting states.
---

# Lint and Fix Loop: Trunk

## Purpose

An autonomous loop for the agent to identify, fix, and verify linting and formatting violations using [Trunk](https://trunk.io).

## Loop Logic

1. **Identify**: Run `make lint` (which executes `trunk check -a`) to list current violations.
2. **Analyze**: Examine the output from Trunk, focusing on the file path, line number, and error message.
3. **Fix**:
   - For formatting issues, run `make format` (which executes `trunk fmt -a`).
   - For linting violations, apply the minimum necessary change to the source code to resolve the error.
4. **Verify**: Re-run `make lint`.
   - If passed: Move to the next issue or finish if all are resolved.
   - If failed: Analyze the new failure and repeat the loop.

## Termination Criteria

- No more errors reported by `make lint`.
- Reached max iteration limit (default: 5).

## Examples

### Scenario: Fixing a formatting violation

1. `make lint` reports formatting issues in `src/your_package/main.py`.
2. Agent runs `make format`.
3. `make lint` now passes.

## Resources

- [Python Development Commands](../common-references/python-commands.md): Common commands for linting and formatting.
- [Trunk Documentation](https://docs.trunk.io/): Official documentation for the Trunk CLI.
