"""Schemaforge Python bindings.

The package uses the native extension when available and falls back to the
``schemaforge`` CLI for validation. Schema-object attribute introspection is
also available without the native extension because it is derived from the
already parsed schema document.
"""

from __future__ import annotations

import json
import subprocess
import sys
from typing import TYPE_CHECKING, TypedDict

if TYPE_CHECKING:
    from typing import Any


class ObjectAttribute(TypedDict):
    """Description of one property declared in a JSON Schema object."""

    name: str
    required: bool
    types: list[str]
    title: str | None
    description: str | None
    format: str | None
    attributes: list[ObjectAttribute]
    schema: Any


try:
    from schemaforge import _native as _ext  # type: ignore[import]

    _NATIVE = True
except ImportError:
    _ext = None  # type: ignore[assignment]
    _NATIVE = False


def validate_json(schema_str: str, instance_str: str) -> list[str]:
    """Validate a JSON instance against a JSON Schema."""
    if _NATIVE:
        return _ext.validate_json(schema_str, instance_str)  # type: ignore[return-value]
    return _cli_validate(schema_str, instance_str)


class CompiledSchema:
    """A compiled JSON Schema for repeated validation and introspection."""

    def __init__(self, schema_str: str) -> None:
        self._schema_str = schema_str
        self._native_handle: Any = None
        try:
            parsed = json.loads(schema_str)
        except json.JSONDecodeError as exc:
            raise ValueError(f"schema_str is not valid JSON: {exc}") from exc
        if not isinstance(parsed, (dict, bool)):
            raise ValueError("schema_str must encode a JSON object or boolean schema")
        self._schema = parsed

        if _NATIVE:
            self._native_handle = _ext.CompiledSchema(schema_str)  # type: ignore[attr-defined]

    def validate_json(self, instance_str: str) -> list[str]:
        """Validate a JSON instance against this compiled schema."""
        if self._native_handle is not None:
            return self._native_handle.validate_json(instance_str)  # type: ignore[return-value]
        return _cli_validate(self._schema_str, instance_str)

    def object_attributes(self) -> list[ObjectAttribute]:
        """Return ordered descriptors for root ``properties`` attributes.

        Descriptors include requiredness, accepted JSON types, annotations,
        nested object attributes, and the property's schema object. ``oneOf``
        and ``anyOf`` variants are intentionally not flattened because that
        would erase their alternative semantics.
        """
        if self._native_handle is not None and hasattr(
            self._native_handle, "object_attributes_json"
        ):
            return json.loads(self._native_handle.object_attributes_json())
        return _object_attributes(self._schema)

    def object_attribute(self, name: str) -> ObjectAttribute | None:
        """Return one root object attribute by its exact JSON property name."""
        return next(
            (attribute for attribute in self.object_attributes() if attribute["name"] == name),
            None,
        )


def compile_schema(schema_str: str) -> CompiledSchema:
    """Compile a JSON Schema into a reusable handle."""
    return CompiledSchema(schema_str)


def _schema_types(schema: Any) -> list[str]:
    if not isinstance(schema, dict):
        return ["null", "boolean", "number", "string", "array", "object"]
    declared = schema.get("type")
    if isinstance(declared, str):
        return [declared]
    if isinstance(declared, list):
        return [item for item in declared if isinstance(item, str)]
    return ["null", "boolean", "number", "string", "array", "object"]


def _object_attributes(schema: Any) -> list[ObjectAttribute]:
    if not isinstance(schema, dict):
        return []
    properties = schema.get("properties")
    if not isinstance(properties, dict):
        return []
    required_value = schema.get("required", [])
    required = set(required_value) if isinstance(required_value, list) else set()

    result: list[ObjectAttribute] = []
    for name, child in properties.items():
        child_schema = child if isinstance(child, (dict, bool)) else {}
        child_dict = child if isinstance(child, dict) else {}
        result.append(
            {
                "name": name,
                "required": name in required,
                "types": _schema_types(child_schema),
                "title": child_dict.get("title")
                if isinstance(child_dict.get("title"), str)
                else None,
                "description": child_dict.get("description")
                if isinstance(child_dict.get("description"), str)
                else None,
                "format": child_dict.get("format")
                if isinstance(child_dict.get("format"), str)
                else None,
                "attributes": _object_attributes(child_schema),
                "schema": child_schema,
            }
        )
    return result


def _cli_validate(schema_str: str, instance_str: str) -> list[str]:
    try:
        json.loads(schema_str)
    except json.JSONDecodeError as exc:
        raise ValueError(f"schema_str is not valid JSON: {exc}") from exc
    try:
        json.loads(instance_str)
    except json.JSONDecodeError as exc:
        raise ValueError(f"instance_str is not valid JSON: {exc}") from exc

    try:
        result = subprocess.run(
            [
                "schemaforge",
                "validate",
                "--schema-json",
                schema_str,
                "--instance-json",
                instance_str,
            ],
            capture_output=True,
            text=True,
            check=False,
        )
    except FileNotFoundError as exc:
        raise RuntimeError(
            "The 'schemaforge' CLI binary was not found on PATH and the "
            "native extension module is not installed. Install the native "
            "wheel or cargo install schemaforge-cli."
        ) from exc

    if result.returncode == 0:
        return []
    lines = [line.strip() for line in result.stderr.splitlines() if line.strip()]
    return lines if lines else [f"validation failed (exit {result.returncode})"]


__all__ = [
    "CompiledSchema",
    "ObjectAttribute",
    "compile_schema",
    "validate_json",
]

__version__ = "0.1.0"


def _native_available() -> bool:
    """Return ``True`` when the native Rust extension is loaded."""
    return _NATIVE


if __name__ == "__main__":  # pragma: no cover
    print(f"schemaforge {__version__} (native={_NATIVE}, python={sys.version})")
