//! Swift AST parser — Phase 2b
//!
//! Parses Swift source into a lightweight `SwiftFile` IR using
//! `tree-sitter-swift`. The IR is then consumed by the classifier
//! (classify.rs) and code generator (codegen.rs) to produce accurate
//! field and parameter information.
//!
//! # Why tree-sitter?
//!
//! - Compiled to WASM / native; no Swift toolchain required on the
//!   build machine.
//! - Fault-tolerant: produces a partial tree even for incomplete files.
//! - Used in production by Neovim, Helix, GitHub Code Search.
//! - The `tree-sitter` crate provides a safe Rust API.
//!
//! Until `tree-sitter-swift` grammar bindings are wired in (Phase 2c),
//! this module provides the `SwiftFile` IR and a regex-based fallback
//! parser that accurately handles fields, parameters, and return types
//! for the common Tier 1 patterns.

use serde::{Deserialize, Serialize};

use crate::types::swift_to_rust;

// ── IR types ───────────────────────────────────────────────────────────────────

/// A parsed Swift field / stored property.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwiftField {
    pub name:      String,
    pub swift_type: String,
    pub optional:  bool,
    pub mutable:   bool,  // `var` vs `let`
}

impl SwiftField {
    /// Map this field to its Rust type string.
    /// Returns `None` when the Swift type has no Tier 1 mapping.
    pub fn rust_type(&self) -> Option<String> {
        let base = swift_to_rust(&self.swift_type)?.to_owned();
        if self.optional {
            Some(format!("Option<{base}>"))
        } else {
            Some(base)
        }
    }
}

/// A parsed Swift function parameter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwiftParam {
    pub label:     Option<String>,   // external label (e.g. `with` in `with value:`)
    pub name:      String,           // internal name
    pub swift_type: String,
    pub optional:  bool,
    pub has_default: bool,
}

impl SwiftParam {
    pub fn rust_type(&self) -> Option<String> {
        let base = swift_to_rust(&self.swift_type)?.to_owned();
        if self.optional {
            Some(format!("Option<{base}>"))
        } else {
            Some(base)
        }
    }

    /// Rust function argument: `name: RustType`
    pub fn as_rust_arg(&self) -> Option<String> {
        self.rust_type().map(|t| format!("{}: {t}", self.name))
    }
}

/// A parsed Swift function / method.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwiftFunc {
    pub name:       String,
    pub params:     Vec<SwiftParam>,
    pub return_type: String,         // Swift return type string
    pub is_async:   bool,
    pub is_throws:  bool,
    pub is_static:  bool,
    pub line:       usize,
}

impl SwiftFunc {
    /// Rust function signature string (without body).
    pub fn rust_signature(&self) -> String {
        let params: Vec<String> = self.params.iter()
            .filter_map(|p| p.as_rust_arg())
            .collect();
        let ret = swift_to_rust(&self.return_type)
            .unwrap_or("() /* UNMAPPED */");
        let throws = if self.is_throws { "/* throws */ " } else { "" };
        format!(
            "pub fn {}({}) {throws}-> {ret}",
            self.name,
            params.join(", ")
        )
    }
}

/// A parsed Swift struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwiftStruct {
    pub name:    String,
    pub fields:  Vec<SwiftField>,
    pub methods: Vec<SwiftFunc>,
    pub line:    usize,
}

/// A parsed Swift enum case.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwiftEnumCase {
    pub name:            String,
    pub associated_types: Vec<String>,
}

/// A parsed Swift enum.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwiftEnum {
    pub name:  String,
    pub cases: Vec<SwiftEnumCase>,
    pub line:  usize,
}

/// A parsed Swift class.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwiftClass {
    pub name:       String,
    pub superclass: Option<String>,
    pub fields:     Vec<SwiftField>,
    pub methods:    Vec<SwiftFunc>,
    pub has_deinit: bool,
    pub line:       usize,
}

/// Top-level parsed representation of a Swift source file.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SwiftFile {
    pub structs:  Vec<SwiftStruct>,
    pub enums:    Vec<SwiftEnum>,
    pub classes:  Vec<SwiftClass>,
    pub funcs:    Vec<SwiftFunc>,  // top-level free functions
}

// ── Fallback regex-based parser ────────────────────────────────────────────────
//
// Handles the common Tier 1 patterns without a full Swift grammar.
// tree-sitter integration will replace this in Phase 2c.

/// Parse a Swift source string into a `SwiftFile` IR.
pub fn parse(source: &str) -> SwiftFile {
    let mut file = SwiftFile::default();
    let lines: Vec<&str> = source.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i].trim();

        // ── struct ────────────────────────────────────────────────────────────
        if is_decl(line, "struct") {
            let name = extract_name(line, "struct");
            let (body, consumed) = extract_body(&lines, i);
            let fields  = parse_fields(&body);
            let methods = parse_methods(&body);
            file.structs.push(SwiftStruct { name, fields, methods, line: i + 1 });
            i += consumed;
            continue;
        }

        // ── enum ──────────────────────────────────────────────────────────────
        if is_decl(line, "enum") {
            let name = extract_name(line, "enum");
            let (body, consumed) = extract_body(&lines, i);
            let cases = parse_enum_cases(&body);
            file.enums.push(SwiftEnum { name, cases, line: i + 1 });
            i += consumed;
            continue;
        }

        // ── class ─────────────────────────────────────────────────────────────
        if is_decl(line, "class") {
            let name = extract_name(line, "class");
            let superclass = extract_superclass(line);
            let (body, consumed) = extract_body(&lines, i);
            let fields     = parse_fields(&body);
            let methods    = parse_methods(&body);
            let has_deinit = body.iter().any(|l| l.trim().starts_with("deinit"));
            file.classes.push(SwiftClass {
                name, superclass, fields, methods, has_deinit, line: i + 1,
            });
            i += consumed;
            continue;
        }

        // ── top-level func ────────────────────────────────────────────────────
        if is_func(line) {
            if let Some(f) = parse_func_line(line, i + 1) {
                file.funcs.push(f);
            }
        }

        i += 1;
    }

    file
}

// ── Body extraction ─────────────────────────────────────────────────────────────

/// Extract the body lines between the opening `{` and matching `}` for a
/// declaration starting at `start_line`. Returns the body lines and the
/// number of source lines consumed (including the declaration line).
fn extract_body<'a>(lines: &[&'a str], start: usize) -> (Vec<&'a str>, usize) {
    let mut depth  = 0i32;
    let mut body   = Vec::new();
    let mut found_open = false;

    for (offset, line) in lines[start..].iter().enumerate() {
        for ch in line.chars() {
            match ch {
                '{' => { depth += 1; found_open = true; }
                '}' => { depth -= 1; }
                _   => {}
            }
        }
        if found_open && offset > 0 {
            body.push(*line);
        }
        if found_open && depth == 0 {
            return (body, offset + 1);
        }
    }
    (body, lines.len() - start)
}

// ── Field parsing ───────────────────────────────────────────────────────────────

fn parse_fields(body: &[&str]) -> Vec<SwiftField> {
    body.iter().filter_map(|line| {
        let t = line.trim();
        let mutable = t.starts_with("var ");
        let is_field = mutable || t.starts_with("let ");
        if !is_field { return None; }
        // skip computed properties (contain `{`)
        if t.contains('{') { return None; }

        let rest = if mutable { &t[4..] } else { &t[4..] };
        // `name: Type` or `name: Type = default`
        let mut parts = rest.splitn(2, ':');
        let name = parts.next()?.trim().to_owned();
        let type_part = parts.next()?.trim();
        // strip default value
        let type_str = type_part.split('=').next()?.trim();
        let optional = type_str.ends_with('?');
        let swift_type = type_str.trim_end_matches('?').trim().to_owned();

        Some(SwiftField { name, swift_type, optional, mutable })
    }).collect()
}

// ── Enum case parsing ──────────────────────────────────────────────────────────

fn parse_enum_cases(body: &[&str]) -> Vec<SwiftEnumCase> {
    body.iter().filter_map(|line| {
        let t = line.trim();
        let rest = t.strip_prefix("case ")?;
        if let Some(paren_start) = rest.find('(') {
            let name = rest[..paren_start].trim().to_owned();
            let inner = &rest[paren_start+1..rest.rfind(')').unwrap_or(rest.len())];
            let associated = inner.split(',').map(|s| {
                s.split(':').last().unwrap_or(s).trim().to_owned()
            }).collect();
            Some(SwiftEnumCase { name, associated_types: associated })
        } else {
            let name = rest.split_whitespace().next()?.to_owned();
            Some(SwiftEnumCase { name, associated_types: vec![] })
        }
    }).collect()
}

// ── Method / func parsing ────────────────────────────────────────────────────────

fn parse_methods(body: &[&str]) -> Vec<SwiftFunc> {
    body.iter().enumerate().filter_map(|(i, line)| {
        if is_func(line.trim()) {
            parse_func_line(line.trim(), i + 1)
        } else {
            None
        }
    }).collect()
}

fn is_func(line: &str) -> bool {
    let line = strip_access(line);
    line.starts_with("func ")
        || line.starts_with("async func ")
        || line.starts_with("static func ")
        || line.starts_with("mutating func ")
}

fn parse_func_line(line: &str, line_num: usize) -> Option<SwiftFunc> {
    let is_async  = line.contains("async ");
    let is_throws = line.contains(" throws");
    let is_static = line.contains("static ");

    let after_func = line.find("func ").map(|i| &line[i+5..])?;
    let name_end   = after_func.find(|c: char| c == '(' || c == '<')?;
    let name       = after_func[..name_end].trim().to_owned();

    let params = if let Some(start) = after_func.find('(') {
        let end = after_func.rfind(')').unwrap_or(after_func.len());
        parse_params(&after_func[start+1..end])
    } else {
        vec![]
    };

    let return_type = if let Some(arrow) = line.find("-> ") {
        let after_arrow = line[arrow+3..].trim();
        // strip trailing `{` or `throws` modifier noise
        after_arrow
            .split(|c: char| c == '{' || c == '/')
            .next().unwrap_or("")
            .trim()
            .to_owned()
    } else {
        "Void".to_owned()
    };

    Some(SwiftFunc {
        name, params, return_type, is_async, is_throws, is_static, line: line_num,
    })
}

fn parse_params(params_str: &str) -> Vec<SwiftParam> {
    if params_str.trim().is_empty() { return vec![]; }
    params_str.split(',').filter_map(|p| {
        let p = p.trim();
        // `label name: Type` or `name: Type` or `_ name: Type`
        let colon = p.find(':')?;
        let lhs   = p[..colon].trim();
        let rhs   = p[colon+1..].trim();

        let parts: Vec<&str> = lhs.split_whitespace().collect();
        let (label, name) = match parts.as_slice() {
            [label, name] if *label == "_" => (None, (*name).to_owned()),
            [label, name]                  => (Some((*label).to_owned()), (*name).to_owned()),
            [name]                         => (None, (*name).to_owned()),
            _                              => return None,
        };

        let has_default  = rhs.contains('=');
        let type_str     = rhs.split('=').next().unwrap_or(rhs).trim();
        let optional     = type_str.ends_with('?');
        let swift_type   = type_str.trim_end_matches('?').trim().to_owned();

        Some(SwiftParam { label, name, swift_type, optional, has_default })
    }).collect()
}

// ── Declaration helpers ──────────────────────────────────────────────────────────

fn strip_access(line: &str) -> &str {
    for prefix in &["public ", "internal ", "private ", "open ", "fileprivate "] {
        if let Some(rest) = line.strip_prefix(prefix) {
            return rest;
        }
    }
    line
}

fn is_decl(line: &str, keyword: &str) -> bool {
    let line = strip_access(line);
    line.starts_with(&format!("{keyword} ")) || line.starts_with(&format!("{keyword}<"))
}

fn extract_name(line: &str, keyword: &str) -> String {
    let line  = strip_access(line);
    let rest  = line.strip_prefix(&format!("{keyword} "))
        .or_else(|| line.strip_prefix(&format!("{keyword}<")))
        .unwrap_or("");
    rest.split(|c: char| !c.is_alphanumeric() && c != '_')
        .next()
        .unwrap_or("Unknown")
        .to_owned()
}

fn extract_superclass(line: &str) -> Option<String> {
    let colon = line.find(':')?;
    let after = line[colon+1..].split('{').next()?.trim();
    let name  = after.split(',').next()?.trim().to_owned();
    if name.is_empty() { None } else { Some(name) }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_struct_with_fields() {
        let src = r#"
struct Point {
    var x: Double
    var y: Double
    let label: String
}
"#;
        let f = parse(src);
        assert_eq!(f.structs.len(), 1);
        let s = &f.structs[0];
        assert_eq!(s.name, "Point");
        assert_eq!(s.fields.len(), 3);
        assert_eq!(s.fields[0].name, "x");
        assert_eq!(s.fields[0].rust_type(), Some("f64".into()));
        assert_eq!(s.fields[2].mutable, false);
    }

    #[test]
    fn parses_enum_with_cases() {
        let src = r#"
enum Direction {
    case north
    case south
    case coordinate(Double, Double)
}
"#;
        let f = parse(src);
        assert_eq!(f.enums.len(), 1);
        let e = &f.enums[0];
        assert_eq!(e.cases.len(), 3);
        assert_eq!(e.cases[2].name, "coordinate");
        assert_eq!(e.cases[2].associated_types.len(), 2);
    }

    #[test]
    fn parses_func_with_params_and_return() {
        let src = "func add(lhs: Int, rhs: Int) -> Int { lhs + rhs }";
        let f = parse(src);
        assert_eq!(f.funcs.len(), 1);
        let func = &f.funcs[0];
        assert_eq!(func.name, "add");
        assert_eq!(func.params.len(), 2);
        assert_eq!(func.return_type, "Int");
        assert_eq!(func.rust_signature(),
            "pub fn add(lhs: i64, rhs: i64) -> i64");
    }

    #[test]
    fn optional_field_maps_to_option() {
        let src = r#"
struct User {
    var name: String
    var email: String?
}
"#;
        let f = parse(src);
        let user = &f.structs[0];
        assert_eq!(user.fields[1].rust_type(), Some("Option<String>".into()));
    }

    #[test]
    fn class_with_deinit_detected() {
        let src = r#"
class Resource {
    var handle: Int
    deinit { close(handle) }
}
"#;
        let f = parse(src);
        assert_eq!(f.classes.len(), 1);
        assert!(f.classes[0].has_deinit);
    }
}
