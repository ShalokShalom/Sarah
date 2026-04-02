# SPEC-008 — tree-sitter-swift Integration Guide

**Status:** Accepted  
**Phase:** 2c (parser upgrade)  
**Authors:** Sarah Architecture Working Group  
**Date:** 2026-04-02  
**Prerequisite:** SPEC-001 (classifier), SPEC-002 (Tier 1 lowering)  
**Related:** `transpiler/src/parser.rs`, `transpiler/Cargo.toml`

---

## 1. Motivation

The Phase 2b parser in `parser.rs` uses a hand-written regex/line-scan
approach. It covers the common Tier 1 patterns correctly, but has hard
limits:

- Nested type expressions (`[[String: Int]]`) are fragile to parse
  line-by-line.
- Attribute chains (`@available`, `@discardableResult`) require extra
  heuristics.
- Multi-line function signatures (parameters split across lines) are not
  handled.
- There is no source-location precision below the line level.

`tree-sitter-swift` is the production-grade Swift grammar used by Neovim,
Helix, and GitHub Code Search. It is fault-tolerant (always returns a
partial tree even for broken input), gives byte-exact source spans, and
has a stable, well-maintained Rust crate. It is the correct replacement
for the Phase 2b regex parser.

---

## 2. Dependency Addition

Add both crates to `transpiler/Cargo.toml`:

```toml
[dependencies]
tree-sitter       = "0.23.0"
tree-sitter-swift = "=0.7.1"
```

**Pin the grammar version with `=`** (exact version specifier). Tree-sitter
parser output is not guaranteed stable across minor bumps — the grammar
crate and the `tree-sitter` runtime must be version-coherent. A semver
range like `"0.7"` risks pulling in a grammar compiled against a different
runtime ABI at the next `cargo update`.

The `tree-sitter-swift` crate ships a `build.rs` that compiles the C
grammar during `cargo build`. No manual `build.rs` changes are needed in
Sarah itself. A C compiler (provided by the `cc` crate, which is a
transitive dependency of `tree-sitter`) is required at build time on all
platforms. The first build is slower by a few seconds; subsequent builds
are fully incremental.

---

## 3. The Rust API

The crate exposes a single `LANGUAGE` constant and a `language()` fn:

```rust
use tree_sitter::{Node, Parser};

let mut parser = Parser::new();
parser
    .set_language(&tree_sitter_swift::LANGUAGE.into())
    .expect("Error loading Swift grammar");

let tree = parser.parse(source, None).unwrap();
let root = tree.root_node();   // kind: "source_file"
```

The returned `Tree` borrows the source string for the lifetime of the
parse. Nodes carry byte offsets and row/column positions, so you can map
any node back to its original source span with:

```rust
fn node_text<'a>(node: &Node, source: &'a str) -> &'a str {
    &source[node.byte_range()]
}
```

Nodes also expose `child_by_field_name()` which is faster and more robust
than positional indexing — the Swift grammar assigns semantic field names
to important children.

---

## 4. Node Types Relevant to Sarah

The grammar's `NODE_TYPES` constant documents every named node. The ones
Sarah's parser needs are:

| Swift construct | tree-sitter node kind | Field names of interest |
|---|---|---|
| `struct Foo { }` | `class_declaration` (with `struct` keyword child) | `name`, `body` |
| `class Bar { }` | `class_declaration` (with `class` keyword child) | `name`, `body`, `type_parameters` |
| `enum E { }` | `enum_declaration` | `name`, `body` |
| `func f()` | `function_declaration` | `name`, `params`, `return_type` |
| `var x: T` | `property_declaration` | `name`, `type_annotation` |
| `let x: T` | `property_declaration` | `name`, `type_annotation` |
| `async func` | `function_declaration` + `async` modifier child | — |
| `deinit { }` | `deinit_declaration` | `body` |
| `case .x(T)` | `enum_case` | `name`, `associated_values` |
| parameter | `parameter` inside `function_value_parameters` | `external_name`, `name`, `type` |
| `protocol P { }` | `protocol_declaration` | `name`, `body` |
| `@objc` | `attribute` child on any declaration | — |
| `associatedtype` | `associatedtype_declaration` | — |

**Struct vs. class disambiguation:** the Swift grammar represents both as
`class_declaration`. Distinguish them by checking whether the first
keyword child is `struct`, `class`, `actor`, or `extension`:

```rust
fn decl_keyword(node: &Node, source: &str) -> &str {
    // The first named child of class_declaration is the keyword token
    node.child(0)
        .map(|c| &source[c.byte_range()])
        .unwrap_or("")
}
```

---

## 5. Integration Architecture

The Phase 2b regex parser is **kept as a fallback**. Tree-sitter is
fault-tolerant (it always returns a partial tree), but for adversarially
malformed input the regex path produces better diagnostic messages.
Sarah gates on a CLI flag:

```
sarah transpile input.swift --parser treesitter   # default
sarah transpile input.swift --parser regex        # Phase 2b fallback
```

```rust
// In parser.rs
pub enum ParserBackend { TreeSitter, Regex }

pub fn parse_with_backend(source: &str, backend: ParserBackend) -> SwiftFile {
    match backend {
        ParserBackend::TreeSitter => parse_with_treesitter(source),
        ParserBackend::Regex      => parse(source),  // Phase 2b
    }
}
```

---

## 6. `parse_with_treesitter()` Implementation

```rust
pub fn parse_with_treesitter(source: &str) -> SwiftFile {
    let mut ts_parser = tree_sitter::Parser::new();
    ts_parser
        .set_language(&tree_sitter_swift::LANGUAGE.into())
        .expect("tree-sitter-swift grammar load failed");

    let tree = ts_parser.parse(source, None)
        .expect("tree-sitter returned None (source too large?)");
    let root = tree.root_node();

    let mut file = SwiftFile::default();
    let mut cursor = root.walk();

    for child in root.children(&mut cursor) {
        if child.is_extra() || !child.is_named() { continue; }
        match child.kind() {
            "class_declaration" => {
                match decl_keyword(&child, source) {
                    "struct" => file.structs.push(extract_struct(&child, source)),
                    "class"  => file.classes.push(extract_class(&child, source)),
                    _        => {} // actor, extension — Tier 3, handled by classifier
                }
            }
            "enum_declaration"      => file.enums.push(extract_enum(&child, source)),
            "function_declaration"  => {
                if let Some(f) = extract_func(&child, source) {
                    file.funcs.push(f);
                }
            }
            _ => {}
        }
    }

    file
}
```

---

## 7. Extractor Reference Implementations

### 7.1 `extract_struct`

```rust
fn extract_struct(node: &Node, source: &str) -> SwiftStruct {
    let name   = node.child_by_field_name("name")
        .map(|n| node_text(&n, source).to_owned())
        .unwrap_or_else(|| "Unknown".to_owned());

    let body_node = node.child_by_field_name("body");
    let fields    = body_node.map(|b| extract_fields(&b, source)).unwrap_or_default();
    let methods   = body_node.map(|b| extract_methods(&b, source)).unwrap_or_default();

    SwiftStruct {
        name,
        fields,
        methods,
        line: node.start_position().row + 1,
    }
}
```

### 7.2 `extract_fields`

```rust
fn extract_fields(body: &Node, source: &str) -> Vec<SwiftField> {
    let mut fields = Vec::new();
    let mut cursor = body.walk();

    for child in body.children(&mut cursor) {
        if child.kind() != "property_declaration" { continue; }

        // `var` vs `let`
        let mutable = child.child(0)
            .map(|c| node_text(&c, source) == "var")
            .unwrap_or(false);

        let name = child.child_by_field_name("name")
            .map(|n| node_text(&n, source).to_owned())
            .unwrap_or_default();

        // type_annotation node: `: TypeExpr`
        let type_node   = child.child_by_field_name("type_annotation");
        let type_text   = type_node
            .and_then(|t| t.child_by_field_name("type"))
            .map(|t| node_text(&t, source).to_owned())
            .unwrap_or_default();

        let optional    = type_text.ends_with('?');
        let swift_type  = type_text.trim_end_matches('?').trim().to_owned();

        // Skip computed properties (body child present)
        if child.child_by_field_name("body").is_some() { continue; }

        fields.push(SwiftField { name, swift_type, optional, mutable });
    }
    fields
}
```

### 7.3 `extract_func`

```rust
fn extract_func(node: &Node, source: &str) -> Option<SwiftFunc> {
    let name = node.child_by_field_name("name")
        .map(|n| node_text(&n, source).to_owned())?;

    // Scan modifier children for `async`, `throws`, `static`
    let modifiers: Vec<&str> = (0..node.child_count())
        .filter_map(|i| node.child(i))
        .filter(|c| c.kind() == "modifier" || c.kind() == "async" || c.kind() == "throws")
        .map(|c| node_text(&c, source))
        .collect();

    let is_async  = modifiers.iter().any(|m| *m == "async")  || node_text(node, source).contains(" async ");
    let is_throws = modifiers.iter().any(|m| *m == "throws") || node_text(node, source).contains(" throws");
    let is_static = modifiers.iter().any(|m| *m == "static");

    // Parameters
    let params = node
        .child_by_field_name("params")
        .map(|p| extract_params(&p, source))
        .unwrap_or_default();

    // Return type
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
```

### 7.4 `extract_params`

```rust
fn extract_params(params_node: &Node, source: &str) -> Vec<SwiftParam> {
    let mut result = Vec::new();
    let mut cursor = params_node.walk();

    for child in params_node.children(&mut cursor) {
        if child.kind() != "parameter" { continue; }

        // External label (may be `_` or absent)
        let external_name = child.child_by_field_name("external_name")
            .map(|n| node_text(&n, source));
        let label = match external_name {
            Some("_") | None => None,
            Some(l)          => Some(l.to_owned()),
        };

        let name = child.child_by_field_name("name")
            .map(|n| node_text(&n, source).to_owned())
            .unwrap_or_default();

        let type_text = child
            .child_by_field_name("type")
            .map(|t| node_text(&t, source).to_owned())
            .unwrap_or_default();

        let optional   = type_text.ends_with('?');
        let swift_type = type_text.trim_end_matches('?').trim().to_owned();
        let has_default = child.child_by_field_name("default_value").is_some();

        result.push(SwiftParam { label, name, swift_type, optional, has_default });
    }
    result
}
```

---

## 8. Error Handling: Partial Trees

Tree-sitter always returns a tree, even for broken Swift. After parsing,
check `root.has_error()` and walk for `ERROR` nodes:

```rust
fn collect_parse_errors(root: &Node, source: &str) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    collect_errors_recursive(root, source, &mut diags);
    diags
}

fn collect_errors_recursive(node: &Node, source: &str, diags: &mut Vec<Diagnostic>) {
    if node.kind() == "ERROR" || node.is_missing() {
        diags.push(Diagnostic {
            code:     "S0-PARSE-ERROR".into(),
            severity: Severity::Warning,
            message:  format!(
                "Syntax error near `{}`; partial tree used — some declarations may be missed.",
                &source[node.byte_range()].chars().take(40).collect::<String>()
            ),
            line: node.start_position().row + 1,
            file: String::new(),
        });
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_errors_recursive(&child, source, diags);
    }
}
```

Sarah's strategy for `S0-PARSE-ERROR`:
1. Emit the diagnostic (Warning severity — not a hard error).
2. Continue extraction from the non-error subtrees.
3. Fall back to the regex parser for any declaration that produced an
   `ERROR` subtree and merge results.

---

## 9. Implementation Plan

| Step | Task | File |
|------|------|------|
| **2c.1** | Add `tree-sitter = "0.23.0"` and `tree-sitter-swift = "=0.7.1"` | `transpiler/Cargo.toml` |
| **2c.2** | Add `ParserBackend` enum and `parse_with_backend()` dispatcher | `transpiler/src/parser.rs` |
| **2c.3** | Implement `parse_with_treesitter()` top-level walker | `transpiler/src/parser.rs` |
| **2c.4** | Implement `extract_struct`, `extract_class`, `extract_enum`, `extract_func`, `extract_params`, `extract_fields` | `transpiler/src/parser.rs` |
| **2c.5** | Implement `collect_parse_errors()` and wire `S0-PARSE-ERROR` diagnostic | `transpiler/src/parser.rs` |
| **2c.6** | Add `--parser` flag to `sarah transpile` and `sarah shell` | `transpiler/src/main.rs` |
| **2c.7** | Extend round-trip tests to run both backends and assert identical `SwiftFile` IR | `transpiler/tests/round_trip.rs` |
| **2c.8** | Flip default to `ParserBackend::TreeSitter` once all tests pass | `transpiler/src/main.rs` |

---

## 10. CI / Build Notes

- The `tree-sitter-swift` build script compiles a C file (`parser.c`)
  on first build. CI images must have a C compiler available. On
  `ubuntu-latest` and `macos-latest` GitHub Actions runners this is
  satisfied by the pre-installed `gcc` / `clang`.
- Add `cargo test --features tree-sitter` (or unconditionally) to the
  CI matrix once step 2c.1 lands.
- Do **not** vendor `parser.c` into the Sarah repository. The `build.rs`
  in the grammar crate handles compilation. Vendoring would break future
  grammar upgrades.
- Apple Silicon (`aarch64-apple-darwin`) is fully supported by both
  `tree-sitter` 0.23 and `tree-sitter-swift` 0.7.

---

## 11. Version Compatibility Table

| `tree-sitter` | `tree-sitter-swift` | Status |
|---|---|---|
| 0.23.x | 0.7.1 | ✅ Tested; use this pair |
| 0.22.x | 0.6.0 | ⚠️ Older API; `LANGUAGE` constant not present |
| 0.20.x | any | ❌ Pre-`Language::into()` API; do not use |

The `LANGUAGE` static (replacing the older `language()` fn) was introduced
in the `tree-sitter` 0.23 / `tree-sitter-swift` 0.7 series. Earlier pairs
require calling `tree_sitter_swift::language()` instead of
`tree_sitter_swift::LANGUAGE.into()`. Sarah targets 0.23+.

---

*This document is part of the Sarah DDD specification set. For the type
mapping used in extracted field/param types, see SPEC-002. For async
bridging of extracted `async func` declarations, see SPEC-006.*
