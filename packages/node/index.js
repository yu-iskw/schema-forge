'use strict';

/**
 * Schemaforge Node.js bindings.
 *
 * Execution strategy:
 *   1. Try to load the native napi-rs extension (`schemaforge-node.*.node`).
 *   2. If that fails, fall back to spawning the `schemaforge` CLI.
 */

const { spawnSync } = require('child_process');
const path = require('path');

// ---------------------------------------------------------------------------
// Try to load the native extension
// ---------------------------------------------------------------------------

let _native = null;

try {
  // napi-rs places the compiled .node file alongside index.js.
  // The exact filename depends on the platform/arch triple.
  // @napi-rs/cli generates a binding loader; when present, use it.
  // eslint-disable-next-line import/no-unresolved
  _native = require('./schemaforge-node.node');
} catch (_err) {
  // Native extension not available; will use CLI fallback.
}

// ---------------------------------------------------------------------------
// CLI subprocess fallback helpers
// ---------------------------------------------------------------------------

/**
 * @param {string} schemaStr
 * @param {string} instanceStr
 * @returns {string[]}
 */
function _cliValidate(schemaStr, instanceStr) {
  // Validate JSON before shelling out.
  try {
    JSON.parse(schemaStr);
  } catch (e) {
    throw new Error(`schemaStr is not valid JSON: ${e.message}`);
  }
  try {
    JSON.parse(instanceStr);
  } catch (e) {
    throw new Error(`instanceStr is not valid JSON: ${e.message}`);
  }

  const result = spawnSync(
    'schemaforge',
    ['validate', '--schema-json', schemaStr, '--instance-json', instanceStr],
    { encoding: 'utf8' }
  );

  if (result.error) {
    throw new Error(
      "The 'schemaforge' CLI binary was not found on PATH and the native " +
        'napi extension is not installed.\n' +
        'Install one of:\n' +
        '  npm install @schemaforge/node   (native)\n' +
        '  cargo install schemaforge-cli   (CLI only)'
    );
  }

  if (result.status === 0) {
    return [];
  }

  const stderr = (result.stderr || '').trim();
  const lines = stderr
    .split('\n')
    .map((l) => l.trim())
    .filter(Boolean);
  return lines.length > 0 ? lines : [`validation failed (exit ${result.status})`];
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/**
 * Validate a JSON instance against a JSON Schema.
 *
 * @param {string} schemaStr  JSON Schema as a JSON string.
 * @param {string} instanceStr  JSON value to validate as a JSON string.
 * @returns {string[]} Empty array when valid; error messages when invalid.
 */
function validateJson(schemaStr, instanceStr) {
  if (_native && typeof _native.validateJson === 'function') {
    return _native.validateJson(schemaStr, instanceStr);
  }
  return _cliValidate(schemaStr, instanceStr);
}

/**
 * A compiled JSON Schema handle for repeated validation.
 */
class CompiledSchema {
  /**
   * @param {string} schemaStr  JSON Schema as a JSON string.
   */
  constructor(schemaStr) {
    this._schemaStr = schemaStr;
    this._native = null;

    if (_native && typeof _native.CompiledSchema === 'function') {
      this._native = new _native.CompiledSchema(schemaStr);
    } else {
      // Validate that the schema is at least parseable JSON.
      try {
        JSON.parse(schemaStr);
      } catch (e) {
        throw new Error(`schemaStr is not valid JSON: ${e.message}`);
      }
    }
  }

  /**
   * @param {string} instanceStr  JSON value to validate as a JSON string.
   * @returns {string[]} Empty array when valid; error messages when invalid.
   */
  validateJson(instanceStr) {
    if (this._native !== null) {
      return this._native.validateJson(instanceStr);
    }
    return _cliValidate(this._schemaStr, instanceStr);
  }
}

/**
 * Compile a JSON Schema string into a CompiledSchema handle.
 *
 * @param {string} schemaStr  JSON Schema as a JSON string.
 * @returns {CompiledSchema}
 */
function compileSchema(schemaStr) {
  return new CompiledSchema(schemaStr);
}

module.exports = { validateJson, compileSchema, CompiledSchema };
