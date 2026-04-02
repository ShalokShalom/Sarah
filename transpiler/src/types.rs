//! Swift → Rust type mapping table — SPEC-002 §3
//!
//! Maps Swift primitive and common standard-library types to their Rust
//! equivalents. This is the canonical reference used by the Tier 1
//! code generator.

/// Map a Swift type name (as it appears in source) to the equivalent Rust type.
///
/// Returns `None` when the type has no direct Tier 1 mapping and must be
/// handled by a higher tier or emitted as a diagnostic.
pub fn swift_to_rust(swift_type: &str) -> Option<&'static str> {
    match swift_type.trim() {
        // Integers
        "Int"    => Some("i64"),
        "Int8"   => Some("i8"),
        "Int16"  => Some("i16"),
        "Int32"  => Some("i32"),
        "Int64"  => Some("i64"),
        "UInt"   => Some("u64"),
        "UInt8"  => Some("u8"),
        "UInt16" => Some("u16"),
        "UInt32" => Some("u32"),
        "UInt64" => Some("u64"),
        // Floats
        "Float"  => Some("f32"),
        "Double" => Some("f64"),
        // Boolean
        "Bool"   => Some("bool"),
        // Text
        "String"    => Some("String"),
        "Character" => Some("char"),
        // Collections
        "[String]"        => Some("Vec<String>"),
        "[Int]"           => Some("Vec<i64>"),
        "[Double]"        => Some("Vec<f64>"),
        "[Bool]"          => Some("Vec<bool>"),
        "[UInt8]"         => Some("Vec<u8>"),
        // Void
        "Void" | "()" | "" => Some("()"),
        // Unknown — caller decides tier
        _ => None,
    }
}

/// Wrap a Rust type in `Option<T>` to represent Swift's `T?` optional.
pub fn make_optional(rust_type: &str) -> String {
    format!("Option<{rust_type}>")
}

/// Map an optional Swift type `T?` to `Option<RustT>`.
/// Returns `None` when the inner type has no Tier 1 mapping.
pub fn swift_optional_to_rust(inner: &str) -> Option<String> {
    swift_to_rust(inner).map(make_optional)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primitives_map_correctly() {
        assert_eq!(swift_to_rust("Int"),    Some("i64"));
        assert_eq!(swift_to_rust("Bool"),   Some("bool"));
        assert_eq!(swift_to_rust("String"), Some("String"));
        assert_eq!(swift_to_rust("Double"), Some("f64"));
    }

    #[test]
    fn optional_wraps_correctly() {
        assert_eq!(swift_optional_to_rust("Int"), Some("Option<i64>".to_owned()));
        assert_eq!(swift_optional_to_rust("Bool"), Some("Option<bool>".to_owned()));
    }

    #[test]
    fn unknown_type_returns_none() {
        assert_eq!(swift_to_rust("MyCustomClass"), None);
    }
}
