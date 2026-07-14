# Python Development Commands

This reference documents the common commands used for Python development in this project, primarily using `make` and `uv`.

## Makefile Commands

The `Makefile` provides a high-level interface for common tasks.

| Command             | Description                                             |
| ------------------- | ------------------------------------------------------- |
| `make setup`        | Set up the Python environment and install dependencies. |
| `make lint`         | Run all linting checks using Trunk.                     |
| `make format`       | Format source code using Trunk.                         |
| `make test`         | Run the unit tests using `pytest`.                      |
| `make build`        | Build the Python package.                               |
| `make clean`        | Clean up build artifacts and caches.                    |
| `make publish`      | Publish the package to PyPI.                            |
| `make test-publish` | Publish the package to TestPyPI.                        |

## Dependency Management (uv)

This project uses `uv` for lightning-fast Python package and environment management.

| Task                     | Command                                                |
| ------------------------ | ------------------------------------------------------ |
| Install dependencies     | `uv sync` (production) or `uv sync --all-extras` (dev) |
| Add a dependency         | `uv add <package>`                                     |
| Remove a dependency      | `uv remove <package>`                                  |
| Upgrade all dependencies | `uv lock --upgrade`                                    |
| Run a command in venv    | `uv run <command>`                                     |

## Testing (pytest)

The test suite is powered by `pytest`.

| Task                         | Command                                    |
| ---------------------------- | ------------------------------------------ |
| Run all tests                | `make test` or `uv run pytest`             |
| Run specific test file       | `uv run pytest tests/path/to/test_file.py` |
| Run tests with matching name | `uv run pytest -k "test_name_pattern"`     |

## Linting and Formatting (Trunk)

We use [Trunk](https://trunk.io) for consistent linting and formatting across different tools (Ruff, Mypy, Shellcheck, etc.).

| Task              | Command                         |
| ----------------- | ------------------------------- |
| Check violations  | `make lint` or `trunk check -a` |
| Fix formatting    | `make format` or `trunk fmt -a` |
| Fix specific file | `trunk check -a <file_path>`    |
