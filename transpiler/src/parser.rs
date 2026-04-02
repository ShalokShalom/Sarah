//! Swift AST parser — Phase 2c
//!
//! Parses Swift source into a `SwiftFile` IR via two backends:
//!
//! - `ParserBackend::TreeSitter` (default) — `tree-sitter-swift` grammar;
//!   fault-tolerant, byte-exact source spans (SPEC-008 §§3–7).
//! - `ParserBackend::Regex` (fallback) — hand-written regex/line-scan;
//!   covers common Tier 1 patterns with better error messages on malformed input.
//!
//! Both backends must produce structurally identical output for valid Swift
//! input (SPEC-009 §3, invariant 6). The round-trip test suite in
//! `tests/round_trip.rs` enforces this.
//!
//! # Why a flag rather than auto-detection?
//!
//! See SPEC-008 §5.1. The short answer: contributors must be able to reproduce
//! a regression against the regex baseline without modifying source code.

use serde::{Deserialize, Serialize};
use std::str::FromStr;

use crate::diagnostics::{Diagnostic, Severity};
use crate::types::swift_to_rust;

// ── Backend selector ──────────────────────────────────────────────────────────

/// Selects the Swift parser backend (SPEC-008 §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ParserBackend {
    #[default]
    TreeSitter,
    Regex,
}

impl FromStr for ParserBackend {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "treesitter" | "tree-sitter" => Ok(ParserBackend::TreeSitter),
            "regex"                      => Ok(ParserBackend::Regex),
            other => Err(format!("unknown parser backend: {other}")),
        }
    }
}

impl clap::ValueEnum for ParserBackend {
    fn value_variants<'a>() -> &'a [Self] {
        &[ParserBackend::TreeSitter, ParserBackend::Regex]
    }
    fn to_possible_value(&self) -> Option<clap::builder::PossibleValue> {
        Some(match self {
            ParserBackend::TreeSitter => clap::builder::PossibleValue::new("treesitter"),
            ParserBackend::Regex      => clap::builder::PossibleValue::new("regex"),
        })
    }
}

/// Parse a Swift source string using the chosen backend.
/// Returns the `SwiftFile` IR and any parse diagnostics.
pub fn parse_with_backend(source: &str, backend: ParserBackend) -> (SwiftFile, Vec<Diagnostic>) {
    match backend {
        ParserBackend::TreeSitter => parse_with_treesitter(source),
        ParserBackend::Regex      => (parse(source), vec![]),
    }
}

// ── IR types ───────────────────────────────────────────────────────────────────

/// A parsed Swift field / stored property (SPEC-009 §2.7).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwiftField {
    pub name:       String,
    pub swift_type: String,
    pub optional:   bool,
    pub mutable:    bool,  // `var` vs `let`
}

impl SwiftField {
    /// Map this field to its Rust type string.
    pub fn rust_type(&self) -> Option<String> {
        let base = swift_to_rust(&self.swift_type)?.to_owned();
        if self.optional { Some(format!("Option<{base}>")) } else { Some(base) }
    }
}

/// A parsed Swift function parameter (SPEC-009 §2.6).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwiftParam {
    pub label:       Option<String>,
    pub name:        String,
    pub swift_type:  String,
    pub optional:    bool,
    pub has_default: bool,
}

impl SwiftParam {
    pub fn rust_type(&self) -> Option<String> {
        let base = swift_to_rust(&self.swift_type)?.to_owned();
        if self.optional { Some(format!("Option<{base}>")) } else { Some(base) }
    }

    pub fn as_rust_arg(&self) -> Option<String> {
        self.rust_type().map(|t| format!("{}: {t}", self.name))
    }
}

/// A parsed Swift function / method (SPEC-009 §2.5).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwiftFunc {
    pub name:        String,
    pub params:      Vec<SwiftParam>,
    pub return_type: String,
    pub is_async:    bool,
    pub is_throws:   bool,
    pub is_static:   bool,
    pub line:        usize,
}

impl SwiftFunc {
    pub fn rust_signature(&self) -> String {
        let params: Vec<String> = self.params.iter()
            .filter_map(|p| p.as_rust_arg())
            .collect();
        let ret    = swift_to_rust(&self.return_type).unwrap_or("() /* UNMAPPED */");
        let throws = if self.is_throws { "/* throws */ " } else { "" };
        format!("pub fn {}({}) {throws}-> {ret}", self.name, params.join(", "))
    }
}

/// A parsed Swift struct (SPEC-009 §2.2).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwiftStruct {
    pub name:    String,
    pub fields:  Vec<SwiftField>,
    pub methods: Vec<SwiftFunc>,
    pub line:    usize,
}

/// A parsed Swift enum case (SPEC-009 §2.3).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwiftEnumCase {
    pub name:             String,
    pub associated_types: Vec<String>,
}

/// A parsed Swift enum (SPEC-009 §2.3).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwiftEnum {
    pub name:  String,
    pub cases: Vec<SwiftEnumCase>,
    pub line:  usize,
}

/// A parsed Swift class (SPEC-009 §2.4).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwiftClass {
    pub name:       String,
    pub superclass: Option<String>,
    pub fields:     Vec<SwiftField>,
    pub methods:    Vec<SwiftFunc>,
    pub has_deinit: bool,
    pub line:       usize,
}

/// Top-level parsed representation of a Swift source file (SPEC-009 §2.1).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SwiftFile {
    pub structs: Vec<SwiftStruct>,
    pub enums:   Vec<SwiftEnum>,
    pub classes: Vec<SwiftClass>,
    pub funcs:   Vec<SwiftFunc>,
}

// ── tree-sitter backend (SPEC-008 §§3–7) ──────────────────────────────────────

/// Parse using tree-sitter-swift. Returns the IR and any parse diagnostics.
pub fn parse_with_treesitter(source: &str) -> (SwiftFile, Vec<Diagnostic>) {
    let mut ts_parser = tree_sitter::Parser::new();
    ts_parser
        .set_language(&tree_sitter_swift::LANGUAGE.into())
        .expect("tree-sitter-swift grammar load failed");

    let tree = ts_parser
        .parse(source, None)
        .expect("tree-sitter returned None (source too large?)");
    let root  = tree.root_node();
    let diags = collect_parse_errors(&root, source);

    let mut file   = SwiftFile::default();
    let mut cursor = root.walk();

    for child in root.children(&mut cursor) {
        if child.is_extra() || !child.is_named() {
            continue;
        }
        match child.kind() {
            "class_declaration" => match decl_keyword(&child, source) {
                "struct" => file.structs.push(extract_struct(&child, source)),
                "class"  => file.classes.push(extract_class(&child, source)),
                _        => {}
            },
            "enum_declaration"     => file.enums.push(extract_enum(&child, source)),
            "function_declaration" => {
                if let Some(f) = extract_func(&child, source) {
                    file.funcs.push(f);
                }
            }
            _ => {}
        }
    }

    (file, diags)
}

// ── tree-sitter helper: keyword disambiguation ────────────────────────────────

fn node_text<'a>(node: &tree_sitter::Node, source: &'a str) -> &'a str {
    &source[node.byte_range()]
}

fn decl_keyword<'a>(node: &tree_sitter::Node, source: &'a str) -> &'a str {
    node.child(0)
        .map(|c| node_text(&c, source))
        .unwrap_or("")
}

// ── tree-sitter extractors (SPEC-008 §7) ──────────────────────────────────────

fn extract_struct(node: &tree_sitter::Node, source: &str) -> SwiftStruct {
    let name = node
        .child_by_field_name("name")
        .map(|n| node_text(&n, source).to_owned())
        .unwrap_or_else(|| "Unknown".to_owned());

    let body_node = node.child_by_field_name("body");
    let fields    = body_node.map(|b| extract_fields_ts(&b, source)).unwrap_or_default();
    let methods   = body_node.map(|b| extract_methods_ts(&b, source)).unwrap_or_default();

    SwiftStruct { name, fields, methods, line: node.start_position().row + 1 }
}

fn extract_class(node: &tree_sitter::Node, source: &str) -> SwiftClass {
    let name = node
        .child_by_field_name("name")
        .map(|n| node_text(&n, source).to_owned())
        .unwrap_or_else(|| "Unknown".to_owned());

    let superclass = extract_superclass_ts(node, source);
    let body_node  = node.child_by_field_name("body");
    let fields     = body_node.map(|b| extract_fields_ts(&b, source)).unwrap_or_default();
    let methods    = body_node.map(|b| extract_methods_ts(&b, source)).unwrap_or_default();
    let has_deinit = body_node.map(|b| {
        let mut cur = b.walk();
        b.children(&mut cur).any(|c| c.kind() == "deinit_declaration")
    }).unwrap_or(false);

    SwiftClass { name, superclass, fields, methods, has_deinit, line: node.start_position().row + 1 }
}

fn extract_enum(node: &tree_sitter::Node, source: &str) -> SwiftEnum {
    let name = node
        .child_by_field_name("name")
        .map(|n| node_text(&n, source).to_owned())
        .unwrap_or_else(|| "Unknown".to_owned());

    let cases = node.child_by_field_name("body")
        .map(|b| {
            let mut cur = b.walk();
            b.children(&mut cur)
                .filter(|c| c.kind() == "enum_entry")
                .map(|c| extract_enum_case(&c, source))
                .collect()
        })
        .unwrap_or_default();

    SwiftEnum { name, cases, line: node.start_position().row + 1 }
}

fn extract_enum_case(node: &tree_sitter::Node, source: &str) -> SwiftEnumCase {
    let name = node
        .child_by_field_name("name")
        .map(|n| node_text(&n, source).to_owned())
        .unwrap_or_default();

    let associated_types = node
        .child_by_field_name("associated_values")
        .map(|av| {
            let mut cur = av.walk();
            av.children(&mut cur)
                .filter(|c| c.kind() == "tuple_type_element" || c.kind() == "type")
                .map(|c| {
                    // The element may have a label: prefer the type child
                    c.child_by_field_name("type")
                        .map(|t| node_text(&t, source).to_owned())
                        .unwrap_or_else(|| node_text(&c, source).to_owned())
                })
                .collect()
        })
        .unwrap_or_default();

    SwiftEnumCase { name, associated_types }
}

fn extract_fields_ts(body: &tree_sitter::Node, source: &str) -> Vec<SwiftField> {
    let mut cur    = body.walk();
    let mut fields = Vec::new();

    for child in body.children(&mut cur) {
        if child.kind() != "property_declaration" {
            continue;
        }
        // Skip computed properties (have a `computed_property` body child)
        if child.child_by_field_name("computed_value").is_some() {
            continue;
        }

        let mutable = child.child(0)
            .map(|c| node_text(&c, source) == "var")
            .unwrap_or(false);

        let name = child
            .child_by_field_name("name")
            .map(|n| node_text(&n, source).to_owned())
            .unwrap_or_default();

        if name.is_empty() {
            continue;
        }

        let type_text = child
            .child_by_field_name("type_annotation")
            .and_then(|ta| ta.child_by_field_name("type"))
            .map(|t| node_text(&t, source).to_owned())
            .unwrap_or_default();

        let optional   = type_text.ends_with('?');
        let swift_type = type_text.trim_end_matches('?').trim().to_owned();

        fields.push(SwiftField { name, swift_type, optional, mutable });
    }
    fields
}

fn extract_methods_ts(body: &tree_sitter::Node, source: &str) -> Vec<SwiftFunc> {
    let mut cur     = body.walk();
    let mut methods = Vec::new();

    for child in body.children(&mut cur) {
        if child.kind() == "function_declaration" {
            if let Some(f) = extract_func(&child, source) {
                methods.push(f);
            }
        }
    }
    methods
}

fn extract_func(node: &tree_sitter::Node, source: &str) -> Option<SwiftFunc> {
    let name = node
        .child_by_field_name("name")
        .map(|n| node_text(&n, source).to_owned())?;

    // Walk children for modifiers
    let full_text  = node_text(node, source);
    let is_async   = full_text.contains(" async ") || full_text.contains("\nasync ");
    let is_throws  = full_text.contains(" throws");
    let is_static  = full_text.contains("static ");

    let params = node
        .child_by_field_name("params")
        .map(|p| extract_params_ts(&p, source))
        .unwrap_or_default();

    let return_type = node
        .child_by_field_name("return_type")
        .map(|r| node_text(&r, source).to_owned())
        .unwrap_or_else(|| "Void".to_owned());

    Some(SwiftFunc {
        name,
        params,
        return_type,
        is_async,
        is_throws,
        is_static,
        line: node.start_position().row + 1,
    })
}

fn extract_params_ts(params_node: &tree_sitter::Node, source: &str) -> Vec<SwiftParam> {
    let mut cur    = params_node.walk();
    let mut result = Vec::new();

    for child in params_node.children(&mut cur) {
        if child.kind() != "parameter" {
            continue;
        }

        let external_name = child
            .child_by_field_name("external_name")
            .map(|n| node_text(&n, source));
        let label = match external_name {
            Some("_") | None => None,
            Some(l)          => Some(l.to_owned()),
        };

        let name = child
            .child_by_field_name("name")
            .map(|n| node_text(&n, source).to_owned())
            .unwrap_or_default();

        let type_text = child
            .child_by_field_name("type")
            .map(|t| node_text(&t, source).to_owned())
            .unwrap_or_default();

        let optional    = type_text.ends_with('?');
        let swift_type  = type_text.trim_end_matches('?').trim().to_owned();
        let has_default = child.child_by_field_name("default_value").is_some();

        result.push(SwiftParam { label, name, swift_type, optional, has_default });
    }
    result
}

fn extract_superclass_ts(node: &tree_sitter::Node, source: &str) -> Option<String> {
    // The `type_inheritance_clause` child contains conformances and superclass
    let inh = (0..node.child_count())
        .filter_map(|i| node.child(i))
        .find(|c| c.kind() == "type_inheritance_clause")?;

    // First `type_identifier` inside the inheritance clause is the superclass
    let mut cur = inh.walk();
    for child in inh.children(&mut cur) {
        if child.kind() == "inheritance_specifier" || child.kind() == "type_identifier" {
            let text = node_text(&child, source).to_owned();
            if !text.is_empty() {
                return Some(text);
            }
        }
    }
    None
}

// ── Error collection (SPEC-008 §8) ────────────────────────────────────────────

pub fn collect_parse_errors(root: &tree_sitter::Node, source: &str) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    collect_errors_recursive(root, source, &mut diags);
    diags
}

fn collect_errors_recursive(
    node: &tree_sitter::Node,
    source: &str,
    diags: &mut Vec<Diagnostic>,
) {
    if node.kind() == "ERROR" || node.is_missing() {
        let snippet: String = source[node.byte_range()]
            .chars()
            .take(40)
            .collect();
        diags.push(Diagnostic {
            code:     "S0-PARSE-ERROR".into(),
            severity: Severity::Warning,
            message:  format!(
                "Syntax error near `{snippet}`; partial tree used — some declarations may be missed."
            ),
            line:     node.start_position().row + 1,
            file:     String::new(),
        });
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_errors_recursive(&child, source, diags);
    }
}

// ── Regex fallback backend ────────────────────────────────────────────────────
//
// Retained as `parse()` for:
//  1. The `--parser regex` CLI flag (SPEC-008 §5)
//  2. Round-trip equivalence tests in `tests/round_trip.rs`
//  3. Fallback for declarations that produced tree-sitter ERROR subtrees

/// Parse a Swift source string using the regex/line-scan fallback.
pub fn parse(source: &str) -> SwiftFile {
    let mut file  = SwiftFile::default();
    let lines: Vec<&str> = source.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i].trim();

        if is_decl(line, "struct") {
            let name = extract_name(line, "struct");
            let (body, consumed) = extract_body(&lines, i);
            let fields  = parse_fields(&body);
            let methods = parse_methods(&body);
            file.structs.push(SwiftStruct { name, fields, methods, line: i + 1 });
            i += consumed;
            continue;
        }

        if is_decl(line, "enum") {
            let name = extract_name(line, "enum");
            let (body, consumed) = extract_body(&lines, i);
            let cases = parse_enum_cases(&body);
            file.enums.push(SwiftEnum { name, cases, line: i + 1 });
            i += consumed;
            continue;
        }

        if is_decl(line, "class") {
            let name       = extract_name(line, "class");
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

        if is_func(line) {
            if let Some(f) = parse_func_line(line, i + 1) {
                file.funcs.push(f);
            }
        }

        i += 1;
    }

    file
}

// ── Body extraction ──────────────────────────────────────────────────────────

fn extract_body<'a>(lines: &[&'a str], start: usize) -> (Vec<&'a str>, usize) {
    let mut depth      = 0i32;
    let mut body       = Vec::new();
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

fn parse_fields(body: &[&str]) -> Vec<SwiftField> {
    body.iter().filter_map(|line| {
        let t       = line.trim();
        let mutable = t.starts_with("var ");
        if !mutable && !t.starts_with("let ") { return None; }
        if t.contains('{') { return None; }
        let rest = &t[4..];
        let mut parts = rest.splitn(2, ':');
        let name      = parts.next()?.trim().to_owned();
        let type_part = parts.next()?.trim();
        let type_str  = type_part.split('=').next()?.trim();
        let optional  = type_str.ends_with('?');
        let swift_type = type_str.trim_end_matches('?').trim().to_owned();
        Some(SwiftField { name, swift_type, optional, mutable })
    }).collect()
}

fn parse_enum_cases(body: &[&str]) -> Vec<SwiftEnumCase> {
    body.iter().filter_map(|line| {
        let t    = line.trim();
        let rest = t.strip_prefix("case ")?;
        if let Some(paren) = rest.find('(') {
            let name  = rest[..paren].trim().to_owned();
            let inner = &rest[paren+1..rest.rfind(')').unwrap_or(rest.len())];
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

fn parse_methods(body: &[&str]) -> Vec<SwiftFunc> {
    body.iter().enumerate().filter_map(|(i, line)| {
        if is_func(line.trim()) { parse_func_line(line.trim(), i + 1) } else { None }
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
    } else { vec![] };

    let return_type = if let Some(arrow) = line.find("-> ") {
        let after_arrow = line[arrow+3..].trim();
        after_arrow
            .split(|c: char| c == '{' || c == '/')
            .next().unwrap_or("")
            .trim()
            .to_owned()
    } else {
        "Void".to_owned()
    };

    Some(SwiftFunc { name, params, return_type, is_async, is_throws, is_static, line: line_num })
}

fn parse_params(params_str: &str) -> Vec<SwiftParam> {
    if params_str.trim().is_empty() { return vec![]; }
    params_str.split(',').filter_map(|p| {
        let p     = p.trim();
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
        let has_default = rhs.contains('=');
        let type_str    = rhs.split('=').next().unwrap_or(rhs).trim();
        let optional    = type_str.ends_with('?');
        let swift_type  = type_str.trim_end_matches('?').trim().to_owned();
        Some(SwiftParam { label, name, swift_type, optional, has_default })
    }).collect()
}

fn strip_access(line: &str) -> &str {
    for prefix in &["public ", "internal ", "private ", "open ", "fileprivate "] {
        if let Some(rest) = line.strip_prefix(prefix) { return rest; }
    }
    line
}

fn is_decl(line: &str, keyword: &str) -> bool {
    let line = strip_access(line);
    line.starts_with(&format!("{keyword} ")) || line.starts_with(&format!("{keyword}<"))
}

fn extract_name(line: &str, keyword: &str) -> String {
    let line = strip_access(line);
    let rest = line.strip_prefix(&format!("{keyword} "))
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
        assert!(!s.fields[2].mutable);
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
        let f   = parse(src);
        assert_eq!(f.funcs.len(), 1);
        let func = &f.funcs[0];
        assert_eq!(func.name, "add");
        assert_eq!(func.params.len(), 2);
        assert_eq!(func.return_type, "Int");
        assert_eq!(func.rust_signature(), "pub fn add(lhs: i64, rhs: i64) -> i64");
    }

    #[test]
    fn optional_field_maps_to_option() {
        let src = r#"
struct User {
    var name: String
    var email: String?
}
"#;
        let f    = parse(src);
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

    #[test]
    fn backend_enum_parses_from_str() {
        assert_eq!(ParserBackend::from_str("treesitter").unwrap(), ParserBackend::TreeSitter);
        assert_eq!(ParserBackend::from_str("regex").unwrap(),      ParserBackend::Regex);
        assert!(ParserBackend::from_str("unknown").is_err());
    }
}
