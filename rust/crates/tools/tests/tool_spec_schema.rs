//! E4-3: Tool spec schema validation tests.
//!
//! Validates that every spec returned by `mvp_tool_specs()` conforms to the
//! expected schema shape:
//!   - non-empty `name`
//!   - non-empty `description`
//!   - `input_schema.type == "object"`
//!   - every field in `input_schema.required` is present in `input_schema.properties`

use tools::mvp_tool_specs;

#[test]
fn all_builtin_tool_specs_have_non_empty_name() {
    let specs = mvp_tool_specs();
    assert!(!specs.is_empty(), "there should be at least one tool spec");
    for spec in &specs {
        assert!(
            !spec.name.is_empty(),
            "tool spec name must not be empty (found empty name)"
        );
    }
}

#[test]
fn all_builtin_tool_specs_have_non_empty_description() {
    for spec in mvp_tool_specs() {
        assert!(
            !spec.description.is_empty(),
            "tool `{}` must have a non-empty description",
            spec.name
        );
    }
}

#[test]
fn all_builtin_tool_specs_have_object_input_schema() {
    for spec in mvp_tool_specs() {
        let schema_type = spec.input_schema.get("type").and_then(|v| v.as_str());
        assert_eq!(
            schema_type,
            Some("object"),
            "tool `{}` input_schema.type should be \"object\", got: {:?}",
            spec.name,
            schema_type
        );
    }
}

#[test]
fn all_builtin_tool_specs_required_fields_are_in_properties() {
    for spec in mvp_tool_specs() {
        let properties = spec.input_schema.get("properties");
        let required = spec.input_schema.get("required").and_then(|v| v.as_array());

        let Some(required_fields) = required else {
            // No required array is acceptable (all fields optional).
            continue;
        };

        let properties_obj = properties.and_then(|v| v.as_object()).unwrap_or_else(|| {
            panic!(
                "tool `{}` has `required` but no `properties` object",
                spec.name
            )
        });

        for req_field in required_fields {
            let field_name = req_field.as_str().unwrap_or_else(|| {
                panic!(
                    "tool `{}` required field should be a string, got: {req_field:?}",
                    spec.name
                )
            });
            assert!(
                properties_obj.contains_key(field_name),
                "tool `{}` has `{}` in required but not in properties",
                spec.name,
                field_name
            );
        }
    }
}

#[test]
fn all_builtin_tool_specs_have_unique_names() {
    let specs = mvp_tool_specs();
    let mut seen = std::collections::BTreeSet::new();
    for spec in &specs {
        assert!(
            seen.insert(spec.name),
            "duplicate tool spec name: `{}`",
            spec.name
        );
    }
}

#[test]
fn mvp_tool_specs_returns_expected_count() {
    // We have at least the core tools documented in ROADMAP (bash, read_file, write_file, etc.)
    // This test catches accidental spec deletions.
    let count = mvp_tool_specs().len();
    assert!(
        count >= 5,
        "expected at least 5 built-in tool specs, got {count}"
    );
}
