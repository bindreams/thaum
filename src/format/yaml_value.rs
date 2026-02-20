/// A lightweight YAML data model for structured emission.
///
/// This is not a general-purpose YAML type — it models exactly the subset
/// of YAML used by the AST output: mappings, sequences, scalars, and null.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum YamlValue {
    /// A scalar string that will be escaped if it contains YAML special chars.
    Scalar(String),
    /// A scalar string that is written verbatim (no escaping). Used for
    /// source locations, type names, and other known-safe values.
    RawScalar(String),
    /// A YAML block scalar (literal `|` style). Used for heredoc bodies.
    BlockScalar(String),
    /// An ordered sequence of values.
    Sequence(Vec<YamlValue>),
    /// An ordered mapping of string keys to values.
    Mapping(Vec<(String, YamlValue)>),
    /// YAML null value. Used in verbose mode for absent optional fields.
    Null,
}

impl YamlValue {
    pub fn scalar(s: impl Into<String>) -> Self {
        YamlValue::Scalar(s.into())
    }

    pub fn block_scalar(s: impl Into<String>) -> Self {
        YamlValue::BlockScalar(s.into())
    }

    pub fn mapping() -> MappingBuilder {
        MappingBuilder::new()
    }
}

/// Builder for constructing YAML mappings incrementally.
pub struct MappingBuilder {
    entries: Vec<(String, YamlValue)>,
}

impl MappingBuilder {
    pub fn new() -> Self {
        MappingBuilder {
            entries: Vec::new(),
        }
    }

    /// Add a key with an escaped scalar value (quotes special chars).
    pub fn scalar(&mut self, key: &str, value: &str) -> &mut Self {
        self.entries
            .push((key.to_string(), YamlValue::Scalar(value.to_string())));
        self
    }

    /// Add a key with a raw scalar value (no escaping). Use for known-safe
    /// values like source locations, type names, and mode strings.
    pub fn raw(&mut self, key: &str, value: &str) -> &mut Self {
        self.entries
            .push((key.to_string(), YamlValue::RawScalar(value.to_string())));
        self
    }

    /// Add a key with an arbitrary YamlValue.
    pub fn value(&mut self, key: &str, value: YamlValue) -> &mut Self {
        self.entries.push((key.to_string(), value));
        self
    }

    /// Conditionally add a raw scalar key-value pair.
    pub fn raw_if(&mut self, cond: bool, key: &str, value: &str) -> &mut Self {
        if cond {
            self.raw(key, value);
        }
        self
    }

    /// Add a key with a null value. Used in verbose mode for absent optionals.
    pub fn null(&mut self, key: &str) -> &mut Self {
        self.entries.push((key.to_string(), YamlValue::Null));
        self
    }

    /// Add a key with a raw boolean value (`true` / `false`).
    pub fn raw_bool(&mut self, key: &str, value: bool) -> &mut Self {
        self.raw(key, if value { "true" } else { "false" })
    }

    /// Add a key with an empty sequence (`[]`).
    pub fn empty_seq(&mut self, key: &str) -> &mut Self {
        self.entries
            .push((key.to_string(), YamlValue::Sequence(Vec::new())));
        self
    }

    /// Build into a YamlValue::Mapping, draining all entries.
    pub fn build(&mut self) -> YamlValue {
        YamlValue::Mapping(std::mem::take(&mut self.entries))
    }
}
