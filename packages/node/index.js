'use strict';

/** Schemaforge Node.js bindings. */

const { spawnSync } = require('child_process');

let _native = null;
try {
  // eslint-disable-next-line import/no-unresolved
  _native = require('./schemaforge-node.node');
} catch (_err) {
  // Native extension not available; validation uses the CLI fallback.
}

function _cliValidate(schemaStr, instanceStr) {
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
        'napi extension is not installed. Install @schemaforge/node or ' +
        'cargo install schemaforge-cli.'
    );
  }
  if (result.status === 0) return [];

  const lines = (result.stderr || '')
    .trim()
    .split('\n')
    .map((line) => line.trim())
    .filter(Boolean);
  return lines.length > 0 ? lines : [`validation failed (exit ${result.status})`];
}

function _schemaTypes(schema) {
  if (!schema || typeof schema !== 'object' || Array.isArray(schema)) {
    return ['null', 'boolean', 'number', 'string', 'array', 'object'];
  }
  if (typeof schema.type === 'string') return [schema.type];
  if (Array.isArray(schema.type)) {
    return schema.type.filter((item) => typeof item === 'string');
  }
  return ['null', 'boolean', 'number', 'string', 'array', 'object'];
}

function _objectAttributes(schema) {
  if (!schema || typeof schema !== 'object' || Array.isArray(schema)) return [];
  if (!schema.properties || typeof schema.properties !== 'object') return [];

  const required = new Set(Array.isArray(schema.required) ? schema.required : []);
  return Object.entries(schema.properties).map(([name, child]) => {
    const childSchema =
      child && (typeof child === 'object' || typeof child === 'boolean') ? child : {};
    const childObject =
      child && typeof child === 'object' && !Array.isArray(child) ? child : {};
    return {
      name,
      required: required.has(name),
      types: _schemaTypes(childSchema),
      title: typeof childObject.title === 'string' ? childObject.title : null,
      description:
        typeof childObject.description === 'string' ? childObject.description : null,
      format: typeof childObject.format === 'string' ? childObject.format : null,
      attributes: _objectAttributes(childSchema),
      schema: childSchema,
    };
  });
}

function validateJson(schemaStr, instanceStr) {
  if (_native && typeof _native.validateJson === 'function') {
    return _native.validateJson(schemaStr, instanceStr);
  }
  return _cliValidate(schemaStr, instanceStr);
}

class CompiledSchema {
  constructor(schemaStr) {
    this._schemaStr = schemaStr;
    this._native = null;
    try {
      this._schema = JSON.parse(schemaStr);
    } catch (e) {
      throw new Error(`schemaStr is not valid JSON: ${e.message}`);
    }
    if (
      typeof this._schema !== 'boolean' &&
      (!this._schema || typeof this._schema !== 'object' || Array.isArray(this._schema))
    ) {
      throw new Error('schemaStr must encode a JSON object or boolean schema');
    }

    if (_native && typeof _native.CompiledSchema === 'function') {
      this._native = new _native.CompiledSchema(schemaStr);
    }
  }

  validateJson(instanceStr) {
    if (this._native !== null) return this._native.validateJson(instanceStr);
    return _cliValidate(this._schemaStr, instanceStr);
  }

  /** Return ordered descriptors for root JSON object properties. */
  objectAttributes() {
    if (
      this._native !== null &&
      typeof this._native.objectAttributesJson === 'function'
    ) {
      return JSON.parse(this._native.objectAttributesJson());
    }
    return _objectAttributes(this._schema);
  }

  /** Return one root attribute by its exact JSON property name. */
  objectAttribute(name) {
    return this.objectAttributes().find((attribute) => attribute.name === name) || null;
  }
}

function compileSchema(schemaStr) {
  return new CompiledSchema(schemaStr);
}

module.exports = { validateJson, compileSchema, CompiledSchema };
