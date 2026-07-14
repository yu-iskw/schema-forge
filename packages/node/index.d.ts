/** Schemaforge Node.js bindings. */

/** Validate a JSON instance against a JSON Schema. */
export function validateJson(schemaStr: string, instanceStr: string): string[];

/** A compiled JSON Schema handle for repeated validation and introspection. */
export class CompiledSchema {
  constructor(schemaStr: string);

  /** Validate a JSON instance against this compiled schema. */
  validateJson(instanceStr: string): string[];
}

/** Compile a JSON Schema string into a reusable handle. */
export function compileSchema(schemaStr: string): CompiledSchema;
