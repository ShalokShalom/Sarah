//! Swift → Rust type mapping — SPEC-002 §3 (Phase 2c: extended)
//!
//! Phase 2c additions:
//! - Generic collection types: Array<T>, Dictionary<K,V>, Set<T>
//! - Nested optionals: T?? → Option<Option<T>>
//! - Result<T, E> → Result<T, E>
//! - Tuple support for small arities
//! - `TypeRef` IR for structured type representations

use std::fmt;

// ── TypeRef ──────────────────────────────────────────────────────────────────
//
// A structured representation of a Swift type, produced by `parse_type()`.
// Replaces bare &str comparison for complex generics.

/// A structured Swift type reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeRef {
    /// A plain named type, e.g. `Int`, `String`, `MyStruct`
    Named(String),
    /// `T?`
    Optional(Box<TypeRef>),
    /// `[T]` / `Array<T>`
    Array(Box<TypeRef>),
    /// `[K: V]` / `Dictionary<K, V>`
    Dictionary(Box<TypeRef>, Box<TypeRef>),
    /// `Set<T>`
    Set(Box<TypeRef>),
    /// `Result<T, E>` (Swift 5)
    Result(Box<TypeRef>, Box<TypeRef>),
    /// `(A, B, ...)` — tuple
    Tuple(Vec<TypeRef>),
    /// `() / Void`
    Void,
    /// Type has no Tier 1 mapping; carries the original Swift text
    Unmapped(String),
}

impl TypeRef {
    /// Convert this `TypeRef` to a Rust type string.
    /// Returns `None` for `Unmapped` variants.
    pub fn to_rust(&self) -> Option<String> {
        match self {
            TypeRef::Void                  => Some("()".to_owned()),
            TypeRef::Named(n)              => swift_to_rust(n).map(|s| s.to_owned()),
            TypeRef::Optional(inner)       => inner.to_rust().map(|t| format!("Option<{t}>")),
            TypeRef::Array(elem)           => elem.to_rust().map(|t| format!("Vec<{t}>")),
            TypeRef::Set(elem)             => elem.to_rust().map(|t| format!("std::collections::HashSet<{t}>")),
            TypeRef::Dictionary(k, v)      => {
                let kr = k.to_rust()?;
                let vr = v.to_rust()?;
                Some(format!("std::collections::HashMap<{kr}, {vr}>"))
            }
            TypeRef::Result(ok, err)       => {
                let okr  = ok.to_rust()?;
                let errr = err.to_rust()?;
                Some(format!("Result<{okr}, {errr}>"))
            }
            TypeRef::Tuple(elems)          => {
                let parts: Option<Vec<_>> = elems.iter().map(|e| e.to_rust()).collect();
                parts.map(|p| format!("({})", p.join(", ")))
            }
            TypeRef::Unmapped(_)           => None,
        }
    }

    /// Returns true if the type can be lowered to Tier 1 Rust.
    pub fn is_tier1(&self) -> bool { self.to_rust().is_some() }

    /// Returns the original Swift text for diagnostics.
    pub fn swift_text(&self) -> String {
        match self {
            TypeRef::Void                    => "Void".to_owned(),
            TypeRef::Named(n)                => n.clone(),
            TypeRef::Optional(i)             => format!("{}?", i.swift_text()),
            TypeRef::Array(e)                => format!("[{}]", e.swift_text()),
            TypeRef::Set(e)                  => format!("Set<{}>", e.swift_text()),
            TypeRef::Dictionary(k, v)        => format!("[{}: {}]", k.swift_text(), v.swift_text()),
            TypeRef::Result(ok, err)         => format!("Result<{}, {}>", ok.swift_text(), err.swift_text()),
            TypeRef::Tuple(elems)            => format!("({})", elems.iter().map(|e| e.swift_text()).collect::<Vec<_>>().join(", ")),
            TypeRef::Unmapped(s)             => s.clone(),
        }
    }
}

impl fmt::Display for TypeRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.swift_text())
    }
}

// ── parse_type ────────────────────────────────────────────────────────────────

/// Parse a Swift type string into a `TypeRef`.
///
/// Handles:
/// - Primitives and named types
/// - Optionals (postfix `?`)
/// - `[T]` array shorthand
/// - `[K: V]` dictionary shorthand
/// - `Array<T>`, `Dictionary<K,V>`, `Set<T>`
/// - `Result<T, E>`
/// - `(A, B, C)` tuples
/// - `Void` and `()`
pub fn parse_type(s: &str) -> TypeRef {
    let s = s.trim();

    // Void
    if s == "Void" || s == "()" || s.is_empty() {
        return TypeRef::Void;
    }

    // Optional: strip trailing `?` (handle multiple: T?? → Optional<Optional<T>>)
    if s.ends_with('?') {
        let inner = parse_type(&s[..s.len()-1]);
        return TypeRef::Optional(Box::new(inner));
    }

    // Tuple: `(A, B, ...)`
    if s.starts_with('(') && s.ends_with(')') {
        let inner = &s[1..s.len()-1];
        let elems = split_generic_args(inner)
            .into_iter()
            .map(|e| parse_type(e.trim()))
            .collect();
        return TypeRef::Tuple(elems);
    }

    // Array shorthand: `[T]`
    if s.starts_with('[') && s.ends_with(']') && !s.contains(':') {
        let inner = parse_type(&s[1..s.len()-1]);
        return TypeRef::Array(Box::new(inner));
    }

    // Dictionary shorthand: `[K: V]`
    if s.starts_with('[') && s.ends_with(']') && s.contains(':') {
        let inner = &s[1..s.len()-1];
        let colon = find_colon_outside_brackets(inner);
        if let Some(pos) = colon {
            let k = parse_type(inner[..pos].trim());
            let v = parse_type(inner[pos+1..].trim());
            return TypeRef::Dictionary(Box::new(k), Box::new(v));
        }
    }

    // Generic types: Name<A, B, ...>
    if let Some(lt) = s.find('<') {
        if s.ends_with('>') {
            let name   = &s[..lt];
            let params = &s[lt+1..s.len()-1];
            let args: Vec<TypeRef> = split_generic_args(params)
                .into_iter()
                .map(|a| parse_type(a.trim()))
                .collect();

            return match name {
                "Array"      if args.len() == 1 =>
                    TypeRef::Array(Box::new(args.into_iter().next().unwrap())),
                "Set"        if args.len() == 1 =>
                    TypeRef::Set(Box::new(args.into_iter().next().unwrap())),
                "Optional"   if args.len() == 1 =>
                    TypeRef::Optional(Box::new(args.into_iter().next().unwrap())),
                "Dictionary" if args.len() == 2 => {
                    let mut it = args.into_iter();
                    let k = it.next().unwrap();
                    let v = it.next().unwrap();
                    TypeRef::Dictionary(Box::new(k), Box::new(v))
                }
                "Result"     if args.len() == 2 => {
                    let mut it = args.into_iter();
                    let ok  = it.next().unwrap();
                    let err = it.next().unwrap();
                    TypeRef::Result(Box::new(ok), Box::new(err))
                }
                // Unknown generic — may still be a user-defined struct
                _ => TypeRef::Unmapped(s.to_owned()),
            };
        }
    }

    // Plain named type
    if swift_to_rust(s).is_some() {
        TypeRef::Named(s.to_owned())
    } else {
        TypeRef::Unmapped(s.to_owned())
    }
}

// ── Primitive mapping table ───────────────────────────────────────────────────

/// Map a plain Swift type name to its Rust equivalent.
/// Only handles non-generic, non-optional types.
/// For full type parsing (generics, optionals), use `parse_type()`.
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
        "Float"   => Some("f32"),
        "Float32" => Some("f32"),
        "Float64" => Some("f64"),
        "Double"  => Some("f64"),
        // Boolean
        "Bool" => Some("bool"),
        // Text
        "String"    => Some("String"),
        "Character" => Some("char"),
        // Data
        "Data"    => Some("Vec<u8>"),
        "NSData"  => Some("Vec<u8>"),
        // Foundation scalars
        "TimeInterval" => Some("f64"),
        "CGFloat"      => Some("f64"),
        // Void
        "Void" | "()" | "" => Some("()"),
        // Unknown
        _ => None,
    }
}

/// Wrap a Rust type in `Option<T>` to represent Swift's `T?`.
pub fn make_optional(rust_type: &str) -> String {
    format!("Option<{rust_type}>")
}

/// Map an optional Swift type `T?` to `Option<RustT>`.
pub fn swift_optional_to_rust(inner: &str) -> Option<String> {
    swift_to_rust(inner).map(make_optional)
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Split a comma-separated generic argument list, respecting nested `<>` and `[]`.
fn split_generic_args(s: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut depth  = 0i32;
    let mut start  = 0;
    for (i, ch) in s.char_indices() {
        match ch {
            '<' | '[' | '(' => depth += 1,
            '>' | ']' | ')' => depth -= 1,
            ',' if depth == 0 => {
                result.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    result.push(&s[start..]);
    result
}

/// Find the position of `:` in a dictionary shorthand `[K: V]` body,
/// skipping nested `<>` and `[]`.
fn find_colon_outside_brackets(s: &str) -> Option<usize> {
    let mut depth = 0i32;
    for (i, ch) in s.char_indices() {
        match ch {
            '<' | '[' | '(' => depth += 1,
            '>' | ']' | ')' => depth -= 1,
            ':' if depth == 0 => return Some(i),
            _ => {}
        }
    }
    None
}

// ── Tests ──────────────────────────────────────────────────────────────────

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
    }

    #[test]
    fn unknown_type_returns_none() {
        assert_eq!(swift_to_rust("MyCustomClass"), None);
    }

    #[test]
    fn parse_array_shorthand() {
        let t = parse_type("[String]");
        assert_eq!(t.to_rust(), Some("Vec<String>".to_owned()));
    }

    #[test]
    fn parse_array_generic() {
        let t = parse_type("Array<Int>");
        assert_eq!(t.to_rust(), Some("Vec<i64>".to_owned()));
    }

    #[test]
    fn parse_dictionary_shorthand() {
        let t = parse_type("[String: Int]");
        assert_eq!(t.to_rust(),
            Some("std::collections::HashMap<String, i64>".to_owned()));
    }

    #[test]
    fn parse_dictionary_generic() {
        let t = parse_type("Dictionary<String, Double>");
        assert_eq!(t.to_rust(),
            Some("std::collections::HashMap<String, f64>".to_owned()));
    }

    #[test]
    fn parse_set() {
        let t = parse_type("Set<UInt8>");
        assert_eq!(t.to_rust(),
            Some("std::collections::HashSet<u8>".to_owned()));
    }

    #[test]
    fn parse_result() {
        let t = parse_type("Result<String, Int>");
        assert_eq!(t.to_rust(), Some("Result<String, i64>".to_owned()));
    }

    #[test]
    fn parse_nested_optional() {
        let t = parse_type("Int??");
        assert_eq!(t.to_rust(), Some("Option<Option<i64>>".to_owned()));
    }

    #[test]
    fn parse_optional_array() {
        let t = parse_type("[String]?");
        assert_eq!(t.to_rust(), Some("Option<Vec<String>>".to_owned()));
    }

    #[test]
    fn parse_tuple() {
        let t = parse_type("(Int, String, Bool)");
        assert_eq!(t.to_rust(), Some("(i64, String, bool)".to_owned()));
    }

    #[test]
    fn parse_void() {
        assert_eq!(parse_type("Void").to_rust(), Some("()".to_owned()));
        assert_eq!(parse_type("()").to_rust(),   Some("()".to_owned()));
        assert_eq!(parse_type("").to_rust(),      Some("()".to_owned()));
    }

    #[test]
    fn unmapped_generic_returns_none() {
        let t = parse_type("CustomGeneric<Foo>");
        assert!(t.to_rust().is_none());
    }

    #[test]
    fn data_maps_to_vec_u8() {
        assert_eq!(swift_to_rust("Data"), Some("Vec<u8>"));
    }
}
