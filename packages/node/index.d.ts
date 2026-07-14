/** Schemaforge Node.js bindings. */

/** Description of one property declared in a JSON Schema object. */
export interface ObjectAttribute {
  /** JSON property name exactly as declared by the schema. */
  name: string;
  /** Whether the containing object lists the property in `required`. */
  required: boolean;
  /** Accepted JSON Schema type names. */
  types: string[];
  title: string | null;
  description: string | null;
  format: string | null;
  /** Recursively described child properties for object-valued attributes. */
  attributes: ObjectAttribute[];
  /** The property schema object. */
  schema: unknown;
}

/** Validate a JSON instance against a JSON Schema. */
export function validateJson(schemaStr: string, instanceStr: string): string[];

/** A compiled JSON Schema handle for repeated validation and introspection. */
export class CompiledSchema {
  constructor(schemaStr: string);

  /** Validate a JSON instance against this compiled schema. */
  validateJson(instanceStr: string): string[];

  /**
   * Return ordered descriptors for root `properties` attributes.
   *
   * `oneOf` and `anyOf` alternatives are not flattened because doing so would
   * erase their variant semantics.
   */
  objectAttributes(): ObjectAttribute[];

  /** Return one root attribute by its exact JSON property name. */
  objectAttribute(name: string): ObjectAttribute | null;
}

/** Compile a JSON Schema string into a reusable handle. */
export function compileSchema(schemaStr: string): CompiledSchema;
