"""Schemaforge Python bindings.

This package provides JSON Schema validation backed by the Schemaforge Rust
compiler.

Two execution paths are supported:

1. **Native extension** – when the package is installed via ``pip install``
   with maturin (``maturin develop`` or a wheel built with the
   ``extension-module`` Cargo feature), the fast native Rust validator is
   used directly through the :mod:`schemaforge._native` extension module.

2. **CLI subprocess fallback** – when the native extension is not available
   the functions shell out to the ``schemaforge`` CLI binary.  This mode is
   useful in CI environments or developer machines where Rust is not
   installed but the CLI is on ``PATH``.

Public functions
----------------
- :func:`validate_json` – validate a JSON instance against a JSON Schema.
- :func:`compile_schema` – compile a schema for repeated validation.

Example
-------
>>> import schemaforge
>>> schemaforge.validate_json('{"type": "string"}', '"hello"')
[]
>>> schemaforge.validate_json('{"type": "string"}', '42')
['value is not of type string']  # errors list (non-empty = invalid)
"""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import tempfile
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from typing import Any


# ---------------------------------------------------------------------------
# Try to import the native extension; fall back to CLI subprocess.
# ---------------------------------------------------------------------------

try:
    from schemaforge import _native as _ext  # type: ignore[import]

    _NATIVE = True
except ImportError:
    _ext = None  # type: ignore[assignment]
    _NATIVE = False


# ---------------------------------------------------------------------------
# Public API
# ---------------------------------------------------------------------------


def validate_json(schema_str: str, instance_str: str) -> list[str]:
    """Validate *instance_str* against *schema_str*.

    Both arguments must be valid JSON strings.

    Returns an empty list when the instance is valid.  Returns a non-empty
    list of human-readable error message strings when the instance is invalid.

    Parameters
    ----------
    schema_str:
        JSON Schema encoded as a JSON string.
    instance_str:
        JSON value to validate, encoded as a JSON string.

    Returns
    -------
    list[str]
        Empty on success; error messages on failure.

    Raises
    ------
    ValueError
        When either argument is not valid JSON or the schema cannot be
        compiled.
    """
    if _NATIVE:
        # The native extension exposes validate_json directly.
        return _ext.validate_json(schema_str, instance_str)  # type: ignore[return-value]

    return _cli_validate(schema_str, instance_str)


class CompiledSchema:
    """A compiled JSON Schema that can validate multiple instances efficiently.

    Prefer :func:`compile_schema` over constructing this class directly.
    """

    def __init__(self, schema_str: str) -> None:
        self._schema_str = schema_str
        self._native_handle: Any = None

        if _NATIVE:
            self._native_handle = _ext.CompiledSchema(schema_str)  # type: ignore[attr-defined]

    def validate_json(self, instance_str: str) -> list[str]:
        """Validate a JSON instance against this compiled schema.

        Parameters
        ----------
        instance_str:
            JSON value to validate, encoded as a JSON string.

        Returns
        -------
        list[str]
            Empty on success; error messages on failure.
        """
        if self._native_handle is not None:
            return self._native_handle.validate_json(instance_str)  # type: ignore[return-value]

        return _cli_validate(self._schema_str, instance_str)


def compile_schema(schema_str: str) -> CompiledSchema:
    """Compile *schema_str* into a :class:`CompiledSchema` handle.

    Compiling once and validating many times is more efficient than calling
    :func:`validate_json` repeatedly for the same schema.

    Parameters
    ----------
    schema_str:
        JSON Schema encoded as a JSON string.

    Returns
    -------
    CompiledSchema
        A handle that can be used to validate instances.

    Raises
    ------
    ValueError
        When *schema_str* is not valid JSON or the schema cannot be compiled.
    """
    return CompiledSchema(schema_str)


# ---------------------------------------------------------------------------
# CLI subprocess fallback
# ---------------------------------------------------------------------------


def _cli_validate(schema_str: str, instance_str: str) -> list[str]:
    """Validate via the ``schemaforge validate`` CLI subcommand.

    The CLI is invoked as a subprocess with the schema and instance written to
    temporary files.  This path is used when the native extension module is not
    available.
    """
    # Pre-validate JSON before shelling out to surface clear error messages.
    try:
        json.loads(schema_str)
    except json.JSONDecodeError as exc:
        raise ValueError(f"schema_str is not valid JSON: {exc}") from exc

    try:
        json.loads(instance_str)
    except json.JSONDecodeError as exc:
        raise ValueError(f"instance_str is not valid JSON: {exc}") from exc

    # Private temp directory (same pattern as packages/node): unpredictable
    # names and a single recursive cleanup in finally.
    tmp_dir = tempfile.mkdtemp(prefix="schemaforge-")
    schema_path = os.path.join(tmp_dir, "schema.json")
    instance_path = os.path.join(tmp_dir, "instance.json")
    try:
        with open(schema_path, "w", encoding="utf-8") as schema_file:
            schema_file.write(schema_str)
        with open(instance_path, "w", encoding="utf-8") as instance_file:
            instance_file.write(instance_str)
        try:
            result = subprocess.run(
                ["schemaforge", "validate", schema_path, instance_path],
                capture_output=True,
                text=True,
                check=False,
            )
        except FileNotFoundError as exc:
            raise RuntimeError(
                "The 'schemaforge' CLI binary was not found on PATH and the "
                "native extension module is not installed.  Install one of:\n"
                "  pip install schemaforge  (native wheel)\n"
                "  cargo install schemaforge-cli  (CLI only)"
            ) from exc
    finally:
        # Best-effort cleanup of the private temp directory.
        shutil.rmtree(tmp_dir, ignore_errors=True)

    if result.returncode == 0:
        return []

    lines = [line.strip() for line in result.stderr.splitlines() if line.strip()]
    return lines if lines else [f"validation failed (exit {result.returncode})"]


__all__ = ["CompiledSchema", "compile_schema", "validate_json"]

# ---------------------------------------------------------------------------
# Module metadata
# ---------------------------------------------------------------------------

__version__ = "0.1.0"

if __name__ == "__main__":  # pragma: no cover
    print(f"schemaforge {__version__} (native={_NATIVE}, python={sys.version})")
