/**
 * Schemaforge Node.js bindings.
 *
 * Provides JSON Schema validation backed by the Schemaforge Rust compiler.
 * When the native napi-rs extension is not built the functions fall back to
 * spawning the `schemaforge` CLI as a child process.
 *
 * @packageDocumentation
 */

/**
 * Validate a JSON instance against a JSON Schema.
 *
 * Both arguments must be valid JSON strings.
 *
 * @param schemaStr  - JSON Schema encoded as a JSON string.
 * @param instanceStr - JSON value to validate, encoded as a JSON string.
 * @returns An empty array when the instance is valid, or a non-empty array of
 *   human-readable error message strings when the instance is invalid.
 * @throws {Error} When either argument is not valid JSON, the schema cannot
 *   be compiled, or the fallback CLI is not available.
 *
 * @example
 * ```ts
 * import { validateJson } from '@schemaforge/node';
 *
 * const errors = validateJson('{"type":"string"}', '"hello"');
 * // errors === []
 *
 * const errors2 = validateJson('{"type":"string"}', '42');
 * // errors2 contains one or more error messages
 * ```
 */
export function validateJson(schemaStr: string, instanceStr: string): string[];

/**
 * A compiled JSON Schema handle for repeated validation.
 *
 * Creating a {@link CompiledSchema} is more efficient than calling
 * {@link validateJson} repeatedly for the same schema.
 *
 * @example
 * ```ts
 * import { compileSchema } from '@schemaforge/node';
 *
 * const schema = compileSchema('{"type":"number","minimum":0}');
 * schema.validateJson('3.14');  // []
 * schema.validateJson('-1');    // ['...']
 * ```
 */
export class CompiledSchema {
  /**
   * @param schemaStr - JSON Schema encoded as a JSON string.
   * @throws {Error} When `schemaStr` is not valid JSON or fails compilation.
   */
  constructor(schemaStr: string);

  /**
   * Validate a JSON instance against this compiled schema.
   *
   * @param instanceStr - JSON value to validate, encoded as a JSON string.
   * @returns Empty array when valid; error messages when invalid.
   * @throws {Error} When `instanceStr` is not valid JSON.
   */
  validateJson(instanceStr: string): string[];
}

/**
 * Compile a JSON Schema string into a {@link CompiledSchema} handle.
 *
 * @param schemaStr - JSON Schema encoded as a JSON string.
 * @returns A {@link CompiledSchema} instance.
 * @throws {Error} When `schemaStr` is not valid JSON or fails compilation.
 */
export function compileSchema(schemaStr: string): CompiledSchema;
