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

    /// Returns stable JSON type names accepted by this set.
    ///
    /// `integer` is omitted when `number` is present because JSON Schema
    /// defines integers as a subset of numbers.
    #[must_use]
    pub fn names(self) -> Vec<&'static str> {
        let mut names = Vec::new();
        if self.null {
            names.push("null");
        }
        if self.boolean {
            names.push("boolean");
        }
        if self.integer && !self.number {
            names.push("integer");
        }
        if self.number {
            names.push("number");
        }
        if self.string {
            names.push("string");
        }
        if self.array {
            names.push("array");
        }
        if self.object {
            names.push("object");
        }
        names
    }

    /// Return the JSON Schema type names present in this set, in a stable
    /// order (object, array, string, number, boolean, null).
    ///
    /// `integer` is intentionally omitted because it is a subset of `number`
    /// and code generators treat them together.
    #[must_use]
    pub fn type_names(self) -> Vec<&'static str> {
        let mut names = Vec::new();
        if self.object {
            names.push("object");
        }
        if self.array {
            names.push("array");
        }
        if self.string {
            names.push("string");
        }
        if self.number {
            names.push("number");
        }
        if self.boolean {
            names.push("boolean");
        }
        if self.null {
            names.push("null");
        }
        names
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
    pub min_length: Option<u64>,
    pub max_length: Option<u64>,
    pub pattern: Option<String>,
    pub format: Option<String>,
}

/// Numeric constraints derived from a schema.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct NumericConstraints {
    pub minimum: Option<NumericBound>,
    pub maximum: Option<NumericBound>,
    pub multiple_of: Option<f64>,
}

/// Array constraints derived from a schema.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArrayConstraints {
    pub min_items: Option<u64>,
    pub max_items: Option<u64>,
    pub unique_items: bool,
    pub min_contains: Option<u64>,
    pub max_contains: Option<u64>,
}

/// Object constraints derived from a schema.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectConstraints {
    pub required: Vec<String>,
    pub min_properties: Option<u64>,
    pub max_properties: Option<u64>,
}

/// Stable, language-neutral metadata for one JSON object property.
///
/// This is the public introspection shape used by Rust and language bindings.
/// It deliberately describes the schema attribute rather than an instance
/// value. Nested object properties are recursively represented by
/// [`Self::attributes`]. The complete compiled child schema remains available
/// in [`Self::schema`] for advanced consumers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObjectAttribute {
    /// JSON property name exactly as declared by the schema.
    pub name: String,
    /// Whether the containing object lists this property in `required`.
    pub required: bool,
    /// Accepted JSON types using standard JSON Schema names.
    pub types: Vec<String>,
    /// Optional schema title.
    pub title: Option<String>,
    /// Optional schema description.
    pub description: Option<String>,
    /// Optional string `format` annotation/assertion.
    pub format: Option<String>,
    /// Nested attributes when this property is itself an object schema.
    pub attributes: Vec<ObjectAttribute>,
    /// Complete compiled schema for the property.
    pub schema: SchemaNode,
}

/// A compiled schema node in the IR.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SchemaNode {
    pub types: TypeSet,
    pub string: StringConstraints,
    pub numeric: NumericConstraints,
    pub array: ArrayConstraints,
    pub object: ObjectConstraints,
    pub properties: IndexMap<String, SchemaNode>,
    pub additional_properties: Option<Box<SchemaNode>>,
    pub items: Option<Box<SchemaNode>>,
    pub prefix_items: Vec<SchemaNode>,
    pub enum_values: Vec<Value>,
    pub const_value: Option<Value>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub id: Option<String>,
    pub defs: IndexMap<String, SchemaNode>,
    pub all_of: Vec<SchemaNode>,
    pub any_of: Vec<SchemaNode>,
    pub one_of: Vec<SchemaNode>,
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

    /// Return descriptors for directly declared JSON object properties.
    ///
    /// The returned order follows the source `properties` object order. This
    /// method intentionally does not flatten `oneOf`/`anyOf` alternatives,
    /// because doing so would erase variant semantics. Consumers can inspect
    /// the complete child [`SchemaNode`] in each descriptor when needed.
    #[must_use]
    pub fn object_attributes(&self) -> Vec<ObjectAttribute> {
        self.properties
            .iter()
            .map(|(name, schema)| ObjectAttribute {
                name: name.clone(),
                required: self.object.required.contains(name),
                types: schema
                    .types
                    .names()
                    .into_iter()
                    .map(str::to_owned)
                    .collect(),
                title: schema.title.clone(),
                description: schema.description.clone(),
                format: schema.string.format.clone(),
                attributes: schema.object_attributes(),
                schema: schema.clone(),
            })
            .collect()
    }

    /// Return one directly declared object attribute by its JSON property name.
    #[must_use]
    pub fn object_attribute(&self, name: &str) -> Option<ObjectAttribute> {
        self.object_attributes()
            .into_iter()
            .find(|attribute| attribute.name == name)
    }

    /// Serialize object attribute descriptors to a JSON value.
    ///
    /// # Errors
    ///
    /// Returns a serialization error if a future custom IR value cannot be
    /// represented by `serde_json`.
    pub fn object_attributes_json(&self) -> Result<Value, serde_json::Error> {
        serde_json::to_value(self.object_attributes())
    }
}

/// The top-level compiled IR for a schema document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SchemaIr {
    pub root: SchemaNode,
    pub dialect_uri: String,
    pub source_digest: String,
    pub source_uri: String,
}

impl SchemaIr {
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

    /// Return root JSON object attributes.
    #[must_use]
    pub fn object_attributes(&self) -> Vec<ObjectAttribute> {
        self.root.object_attributes()
    }

    /// Return one root object attribute by JSON property name.
    #[must_use]
    pub fn object_attribute(&self, name: &str) -> Option<ObjectAttribute> {
        self.root.object_attribute(name)
    }

    /// Return root object attributes as JSON.
    ///
    /// # Errors
    ///
    /// Returns an error when serialization fails.
    pub fn object_attributes_json(&self) -> Result<Value, serde_json::Error> {
        self.root.object_attributes_json()
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
        assert_eq!(ts.names(), vec!["number"]);
    }

    #[test]
    fn type_set_any_and_none() {
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

    #[test]
    fn object_attributes_include_required_metadata_and_nested_attributes() {
        let mut root = SchemaNode {
            types: TypeSet::from_json(&json!("object")),
            ..SchemaNode::default()
        };
        root.object.required.push("id".to_owned());

        let mut id = SchemaNode {
            types: TypeSet::from_json(&json!("string")),
            title: Some("Identifier".to_owned()),
            string: crate::StringConstraints {
                format: Some("uuid".to_owned()),
                ..Default::default()
            },
            ..SchemaNode::default()
        };
        let _ = &mut id; // prevent unused_mut lint since we only build this inline

        let mut profile = SchemaNode {
            types: TypeSet::from_json(&json!("object")),
            ..SchemaNode::default()
        };
        let display_name = SchemaNode {
            types: TypeSet::from_json(&json!("string")),
            ..SchemaNode::default()
        };
        profile
            .properties
            .insert("displayName".to_owned(), display_name);

        root.properties.insert("id".to_owned(), id);
        root.properties.insert("profile".to_owned(), profile);

        let attributes = root.object_attributes();
        assert_eq!(attributes.len(), 2);
        assert_eq!(attributes[0].name, "id");
        assert!(attributes[0].required);
        assert_eq!(attributes[0].types, vec!["string"]);
        assert_eq!(attributes[0].format.as_deref(), Some("uuid"));
        assert_eq!(attributes[1].attributes[0].name, "displayName");
    }

    #[test]
    fn object_attributes_can_be_serialized_to_json() {
        let mut root = SchemaNode::default();
        root.properties
            .insert("name".to_owned(), SchemaNode::default());
        let value = root.object_attributes_json().unwrap();
        assert_eq!(value[0]["name"], "name");
        assert!(value[0].get("schema").is_some());
    }
}
