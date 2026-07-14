# Troubleshooting Knowledge Base

This document provides resolutions for common errors encountered during build, lint, and test cycles.

## Python Errors

### ModuleNotFoundError: No module named '...'

- **Cause**: Missing dependency or incorrect import path.
- **Fix**: Ensure the package is in `pyproject.toml` and run `uv sync`. If it's a local module, check the `PYTHONPATH` or the source directory structure.

### ImportError: cannot import name '...' from '...'

- **Cause**: Circular imports or mismatch between package structure and import statement.
- **Fix**: Check for circular dependencies. Ensure the module exists and the name is spelled correctly.

### AttributeError: 'module' object has no attribute '...'

- **Cause**: Trying to access a non-existent attribute in a module, often due to naming conflicts (e.g., a file named `math.py` shadowing the standard library).
- **Fix**: Rename conflicting files or check the module's `__init__.py`.

## uv & Dependencies

### Missing Dependencies in Virtual Environment

- **Cause**: Out-of-sync `uv.lock`.
- **Fix**: Run `uv sync --all-extras`. If issues persist, try a "hard reset" using the `clean-project` skill.

## Trunk & Linting

### Trunk Timeout

- **Cause**: Large codebase or heavy linter (like `trivy`).
- **Fix**: Increase timeout in `.trunk/trunk.yaml` or run with `--all` only when necessary.

### Trunk Environment Errors

- **Cause**: Required runtime (Python version) mismatch.
- **Fix**: Ensure Python version matches `.python-version`. Run `trunk install` to refresh hermetic tools.

## Unit Tests (Pytest)

### Assertion Errors

- **Cause**: Logic error or unexpected data.
- **Fix**: Review the expected vs actual values in the `pytest` output. Add print statements or use `pytest --pdb` for debugging.

### Fixture Errors

- **Cause**: Missing or incorrectly defined pytest fixtures.
- **Fix**: Ensure fixtures are defined in `conftest.py` or the test file and are correctly requested by the test functions.
