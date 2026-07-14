//! Intermediate representation (IR) for compiled JSON Schemas.
//!
//! The IR is a structured, analysed form of a JSON Schema that downstream
//! consumers (code generators, analysers, documentation builders) work with
//! instead of raw JSON.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Bitmask of JSON primitive types that a schema accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct TypeSet {
    /// Accepts JSON `null`.
    pub null: bool,
    /// Accepts JSON `true`/`false`.
    pub boolean: bool,
    /// Accepts JSON integers (subset of `number`).
    pub integer: bool,
    /// Accepts JSON numbers (including non-integer).
    pub number: bool,
    /// Accepts JSON strings.
    pub string: bool,
    /// Accepts JSON arrays.
    pub array: bool,
    /// Accepts JSON objects.
    pub object: bool,
}

impl TypeSet {
    /// A [`TypeSet`] that accepts any JSON type.
    #[must_use]
    pub const fn any() -> Self {
        Self {
            null: true,
            boolean: true,
            integer: true,
            number: true,
            string: true,
            array: true,
            object: true,
        }
    }

    /// A [`TypeSet`] that accepts no JSON types (schema always fails).
    #[must_use]
    pub const fn none() -> Self {
        Self {
            null: false,
            boolean: false,
            integer: false,
            number: false,
            string: false,
            array: false,
            object: false,
        }
    }

    /// Parse from a JSON Schema `type` keyword value.
    ///
    /// Accepts either a string or an array of strings.
    #[must_use]
    pub fn from_json(v: &Value) -> Self {
        let mut set = Self::none();
        match v {
            Value::String(s) => set.apply_str(s),
            Value::Array(arr) => {
                for item in arr {
                    if let Value::String(s) = item {
                        set.apply_str(s);
                    }
                }
            }
            _ => {}
        }
        set
    }

    fn apply_str(&mut self, s: &str) {
        match s {
            "null" => self.null = true,
            "boolean" => self.boolean = true,
            "integer" => self.integer = true,
            "number" => {
                self.number = true;
                self.integer = true;
            }
            "string" => self.string = true,
            "array" => self.array = true,
            "object" => self.object = true,
            _ => {}
        }
    }

    /// Returns `true` when no type is set.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        !self.null
            && !self.boolean
            && !self.integer
            && !self.number
            && !self.string
            && !self.array
            && !self.object
    }
}

/// Numeric constraint bounds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NumericBound {
    /// The bound value.
    pub value: f64,
    /// Whether the bound is exclusive.
    pub exclusive: bool,
}

/// String-level constraints derived from a schema.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StringConstraints {
    /// `minLength` keyword value.
    pub min_length: Option<u64>,
    /// `maxLength` keyword value.
    pub max_length: Option<u64>,
    /// `pattern` keyword value (raw regex string).
    pub pattern: Option<String>,
    /// `format` keyword value.
    pub format: Option<String>,
}

/// Numeric constraints derived from a schema.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct NumericConstraints {
    /// Lower bound (from `minimum` or `exclusiveMinimum`).
    pub minimum: Option<NumericBound>,
    /// Upper bound (from `maximum` or `exclusiveMaximum`).
    pub maximum: Option<NumericBound>,
    /// `multipleOf` keyword value.
    pub multiple_of: Option<f64>,
}

/// Array constraints derived from a schema.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArrayConstraints {
    /// `minItems` keyword value.
    pub min_items: Option<u64>,
    /// `maxItems` keyword value.
    pub max_items: Option<u64>,
    /// `uniqueItems` keyword value.
    pub unique_items: bool,
    /// `minContains` keyword value.
    pub min_contains: Option<u64>,
    /// `maxContains` keyword value.
    pub max_contains: Option<u64>,
}

/// Object constraints derived from a schema.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectConstraints {
    /// `required` keyword value.
    pub required: Vec<String>,
    /// `minProperties` keyword value.
    pub min_properties: Option<u64>,
    /// `maxProperties` keyword value.
    pub max_properties: Option<u64>,
}

/// A compiled schema node in the IR.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SchemaNode {
    /// The inferred set of JSON types this schema accepts.
    pub types: TypeSet,
    /// String-specific constraints.
    pub string: StringConstraints,
    /// Numeric constraints.
    pub numeric: NumericConstraints,
    /// Array constraints.
    pub array: ArrayConstraints,
    /// Object constraints.
    pub object: ObjectConstraints,
    /// Named properties (for object schemas).
    pub properties: IndexMap<String, SchemaNode>,
    /// Schema for additional properties (if constrained).
    pub additional_properties: Option<Box<SchemaNode>>,
    /// Schema for array items.
    pub items: Option<Box<SchemaNode>>,
    /// Schemas for a fixed-length tuple prefix (`prefixItems`).
    pub prefix_items: Vec<SchemaNode>,
    /// `enum` keyword: list of allowed constant values.
    pub enum_values: Vec<Value>,
    /// `const` keyword: single allowed value.
    pub const_value: Option<Value>,
    /// `title` metadata.
    pub title: Option<String>,
    /// `description` metadata.
    pub description: Option<String>,
    /// Raw `$id` of this schema node (if set).
    pub id: Option<String>,
    /// `$defs` / `definitions` sub-schemas keyed by name.
    pub defs: IndexMap<String, SchemaNode>,
    /// `allOf` sub-schemas.
    pub all_of: Vec<SchemaNode>,
    /// `anyOf` sub-schemas.
    pub any_of: Vec<SchemaNode>,
    /// `oneOf` sub-schemas.
    pub one_of: Vec<SchemaNode>,
    /// `not` schema.
    pub not: Option<Box<SchemaNode>>,
}

impl Default for SchemaNode {
    fn default() -> Self {
        Self {
            types: TypeSet::any(),
            string: StringConstraints::default(),
            numeric: NumericConstraints::default(),
            array: ArrayConstraints::default(),
            object: ObjectConstraints::default(),
            properties: IndexMap::new(),
            additional_properties: None,
            items: None,
            prefix_items: Vec::new(),
            enum_values: Vec::new(),
            const_value: None,
            title: None,
            description: None,
            id: None,
            defs: IndexMap::new(),
            all_of: Vec::new(),
            any_of: Vec::new(),
            one_of: Vec::new(),
            not: None,
        }
    }
}

impl SchemaNode {
    /// A schema node that accepts any value.
    #[must_use]
    pub fn any() -> Self {
        Self::default()
    }

    /// A boolean schema: `true` → any, `false` → never.
    #[must_use]
    pub fn boolean_schema(valid: bool) -> Self {
        if valid {
            Self::any()
        } else {
            Self {
                types: TypeSet::none(),
                ..Self::default()
            }
        }
    }

    /// Returns `true` when the schema can never be satisfied.
    #[must_use]
    pub const fn is_never(&self) -> bool {
        self.types.is_empty() && self.enum_values.is_empty() && self.const_value.is_none()
    }
}

/// The top-level compiled IR for a schema document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SchemaIr {
    /// The root schema node.
    pub root: SchemaNode,
    /// The dialect URI detected in the source.
    pub dialect_uri: String,
    /// SHA-256 hex digest of the source bytes.
    pub source_digest: String,
    /// The source URI / file path.
    pub source_uri: String,
}

impl SchemaIr {
    /// Create a new IR with the given root and metadata.
    #[must_use]
    pub fn new(
        root: SchemaNode,
        dialect_uri: impl Into<String>,
        source_digest: impl Into<String>,
        source_uri: impl Into<String>,
    ) -> Self {
        Self {
            root,
            dialect_uri: dialect_uri.into(),
            source_digest: source_digest.into(),
            source_uri: source_uri.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn type_set_from_string() {
        let ts = TypeSet::from_json(&json!("string"));
        assert!(ts.string);
        assert!(!ts.number);
    }

    #[test]
    fn type_set_from_array() {
        let ts = TypeSet::from_json(&json!(["string", "null"]));
        assert!(ts.string);
        assert!(ts.null);
        assert!(!ts.boolean);
    }

    #[test]
    fn type_set_number_implies_integer() {
        let ts = TypeSet::from_json(&json!("number"));
        assert!(ts.number);
        assert!(ts.integer);
    }

    #[test]
    fn type_set_any_and_none() {
        assert!(!TypeSet::none().is_empty() || TypeSet::none().is_empty());
        assert!(TypeSet::none().is_empty());
        assert!(!TypeSet::any().is_empty());
    }

    #[test]
    fn schema_node_default_is_any() {
        let node = SchemaNode::default();
        assert!(node.types.string);
        assert!(node.types.number);
    }

    #[test]
    fn boolean_schema_false_is_never() {
        let never = SchemaNode::boolean_schema(false);
        assert!(never.is_never());
    }
}
