//! Round-trip parser equivalence tests (SPEC-008 §9, step 2c.7)
//!
//! Each test case runs the same Swift source through both
//! `ParserBackend::Regex` and `ParserBackend::TreeSitter` and asserts
//! that the resulting `SwiftFile` IR is structurally identical.
//!
//! SPEC-009 §3 invariant 6: both backends must produce identical output
//! for valid Swift input. A divergence is a bug in whichever backend
//! produces the non-canonical result.
//!
//! Adding a new regression: paste the Swift input as a new `rt!()` call.
//! The test name should identify the pattern being covered.

use sarah_cli_lib::parser::{parse, parse_with_backend, ParserBackend, SwiftFile};

// ── Equivalence helper ───────────────────────────────────────────────────

fn both_backends(source: &str) -> (SwiftFile, SwiftFile) {
    let (ts, _)    = parse_with_backend(source, ParserBackend::TreeSitter);
    let (regex, _) = parse_with_backend(source, ParserBackend::Regex);
    (ts, regex)
}

/// Assert two SwiftFile IRs are structurally identical.
/// Uses JSON serialisation so the comparison is field-by-field and
/// produces a readable diff in the failure message.
fn assert_ir_eq(ts: &SwiftFile, regex: &SwiftFile, label: &str) {
    let ts_json    = serde_json::to_string_pretty(ts).unwrap();
    let regex_json = serde_json::to_string_pretty(regex).unwrap();
    assert_eq!(
        ts_json, regex_json,
        "Backend divergence in `{label}`:\nTreeSitter:\n{ts_json}\nRegex:\n{regex_json}"
    );
}

// ── Test corpus ─────────────────────────────────────────────────────────────

#[test]
fn rt_empty_file() {
    let (ts, regex) = both_backends("");
    assert_ir_eq(&ts, &regex, "empty file");
}

#[test]
fn rt_simple_struct() {
    let src = r#"
struct Point {
    var x: Double
    var y: Double
    let label: String
}
"#;
    let (ts, regex) = both_backends(src);
    assert_ir_eq(&ts, &regex, "simple struct");
    assert_eq!(ts.structs.len(), 1);
    assert_eq!(ts.structs[0].fields.len(), 3);
}

#[test]
fn rt_optional_field() {
    let src = r#"
struct User {
    var name: String
    var email: String?
    let id: Int
}
"#;
    let (ts, regex) = both_backends(src);
    assert_ir_eq(&ts, &regex, "optional field");
    assert!(ts.structs[0].fields[1].optional);
    assert!(!ts.structs[0].fields[0].optional);
}

#[test]
fn rt_simple_enum() {
    let src = r#"
enum Direction {
    case north
    case south
    case east
    case west
}
"#;
    let (ts, regex) = both_backends(src);
    assert_ir_eq(&ts, &regex, "simple enum");
    assert_eq!(ts.enums[0].cases.len(), 4);
}

#[test]
fn rt_enum_with_associated_values() {
    let src = r#"
enum Shape {
    case circle(Double)
    case rectangle(Double, Double)
    case point
}
"#;
    let (ts, regex) = both_backends(src);
    assert_ir_eq(&ts, &regex, "enum with associated values");
    assert_eq!(ts.enums[0].cases[0].associated_types.len(), 1);
    assert_eq!(ts.enums[0].cases[1].associated_types.len(), 2);
    assert!(ts.enums[0].cases[2].associated_types.is_empty());
}

#[test]
fn rt_simple_class() {
    let src = r#"
class Counter {
    var count: Int
    let name: String
}
"#;
    let (ts, regex) = both_backends(src);
    assert_ir_eq(&ts, &regex, "simple class");
    assert_eq!(ts.classes[0].fields.len(), 2);
    assert!(!ts.classes[0].has_deinit);
}

#[test]
fn rt_class_with_deinit() {
    let src = r#"
class Resource {
    var handle: Int
    deinit { closeHandle(handle) }
}
"#;
    let (ts, regex) = both_backends(src);
    assert_ir_eq(&ts, &regex, "class with deinit");
    assert!(ts.classes[0].has_deinit);
}

#[test]
fn rt_top_level_func_no_params() {
    let src = "func greet() {}";
    let (ts, regex) = both_backends(src);
    assert_ir_eq(&ts, &regex, "top-level func no params");
    assert_eq!(ts.funcs[0].name, "greet");
    assert!(ts.funcs[0].params.is_empty());
    assert_eq!(ts.funcs[0].return_type, "Void");
}

#[test]
fn rt_func_with_params_and_return() {
    let src = "func add(lhs: Int, rhs: Int) -> Int { lhs + rhs }";
    let (ts, regex) = both_backends(src);
    assert_ir_eq(&ts, &regex, "func with params and return");
    assert_eq!(ts.funcs[0].params.len(), 2);
    assert_eq!(ts.funcs[0].return_type, "Int");
}

#[test]
fn rt_async_func() {
    let src = "async func fetchData() throws -> String {}";
    let (ts, regex) = both_backends(src);
    assert_ir_eq(&ts, &regex, "async throws func");
    assert!(ts.funcs[0].is_async);
    assert!(ts.funcs[0].is_throws);
}

#[test]
fn rt_static_func() {
    let src = "static func make() -> Int { 0 }";
    let (ts, regex) = both_backends(src);
    assert_ir_eq(&ts, &regex, "static func");
    assert!(ts.funcs[0].is_static);
}

#[test]
fn rt_func_external_label() {
    let src = "func move(to destination: String) {}";
    let (ts, regex) = both_backends(src);
    assert_ir_eq(&ts, &regex, "func external label");
    let param = &ts.funcs[0].params[0];
    assert_eq!(param.label.as_deref(), Some("to"));
    assert_eq!(param.name, "destination");
}

#[test]
fn rt_func_underscore_label() {
    let src = "func log(_ message: String) {}";
    let (ts, regex) = both_backends(src);
    assert_ir_eq(&ts, &regex, "func underscore label");
    assert!(ts.funcs[0].params[0].label.is_none());
}

#[test]
fn rt_class_with_methods() {
    let src = r#"
class SessionManager {
    var token: String
    func login(user: String) {}
    async func refresh() throws {}
}
"#;
    let (ts, regex) = both_backends(src);
    assert_ir_eq(&ts, &regex, "class with methods");
    assert_eq!(ts.classes[0].methods.len(), 2);
    assert!(ts.classes[0].methods[1].is_async);
}

#[test]
fn rt_struct_with_methods() {
    let src = r#"
struct Vector {
    var x: Double
    var y: Double
    func length() -> Double { 0.0 }
}
"#;
    let (ts, regex) = both_backends(src);
    assert_ir_eq(&ts, &regex, "struct with methods");
    assert_eq!(ts.structs[0].methods.len(), 1);
}

#[test]
fn rt_multiple_declarations() {
    let src = r#"
struct Config {
    var timeout: Int
}

enum Status {
    case active
    case inactive
}

class Manager {
    var config: Config
}

func start() {}
"#;
    let (ts, regex) = both_backends(src);
    assert_ir_eq(&ts, &regex, "multiple declarations");
    assert_eq!(ts.structs.len(), 1);
    assert_eq!(ts.enums.len(), 1);
    assert_eq!(ts.classes.len(), 1);
    assert_eq!(ts.funcs.len(), 1);
}

#[test]
fn rt_public_access_modifier() {
    let src = r#"
public struct Point {
    public var x: Double
    public var y: Double
}
"#;
    let (ts, regex) = both_backends(src);
    assert_ir_eq(&ts, &regex, "public access modifiers");
    assert_eq!(ts.structs[0].name, "Point");
}
