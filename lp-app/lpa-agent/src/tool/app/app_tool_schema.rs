//! [`app_tool_schema`]: a tool's JSON input schema, generated from the type
//! the tool deserializes — so the schema the model sees and the parser that
//! judges its input cannot disagree.

use schemars::JsonSchema;
use schemars::generate::SchemaSettings;
use serde_json::Value;

/// The input schema for `T`, with every subschema inlined (some
/// OpenAI-compatible servers do not resolve `$ref`) and no `$schema` key.
pub fn app_tool_schema<T: JsonSchema>() -> Value {
    let generator = SchemaSettings::draft2020_12()
        .with(|settings| settings.inline_subschemas = true)
        .into_generator();
    let schema = generator.into_root_schema_for::<T>();
    let mut value = serde_json::to_value(schema).expect("a schema serializes");
    if let Some(map) = value.as_object_mut() {
        map.remove("$schema");
        map.remove("title");
    }
    value
}
