# SPEC-009 — SwiftFile IR: Definition, Contract, and Versioning

**Status:** Accepted  
**Version:** 1.0.0  
**Date:** 2026-04-02  
**Authors:** Sarah Architecture Working Group  
**Phase:** Cross-cutting (applies to all phases)  
**Related:** SPEC-001 (classifier), SPEC-002 (lowering), SPEC-003 (classes),
 SPEC-006 (async), SPEC-008 (parser), `transpiler/src/parser.rs`,
 `transpiler/src/classify.rs`, `transpiler/src/codegen.rs`,
 `transpiler/src/shell_gen.rs`

---

## 1. Purpose

`SwiftFile` is the **canonical intermediate representation** that flows
between every stage of Sarah's transpilation pipeline:

```
Swift source
    │
    ▼  parser.rs  (produces)
 SwiftFile IR
    ├──▼  classify.rs  (reads, emits tier annotations)
    ├──▼  codegen.rs   (reads, emits Rust Core source)
    └──▼  shell_gen.rs (reads, emits Swift Shell source)
```

Because every major subsystem depends on `SwiftFile`, changes to its
shape have wide blast radius. This spec defines the type, its stability
contract, and the rules contributors must follow when modifying it.

---

## 2. Canonical Type Definition

The authoritative Rust definition lives in `transpiler/src/parser.rs`.
The following is the normative record of the type as of SPEC-009 v1.0.0.
If `parser.rs` and this document disagree, `parser.rs` is the source of
truth and this spec must be updated in the same PR.

### 2.1 `SwiftFile` (top-level)

```rust
/// Top-level parsed representation of a Swift source file.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SwiftFile {
    pub structs:  Vec<SwiftStruct>,   // top-level struct declarations
    pub enums:    Vec<SwiftEnum>,     // top-level enum declarations
    pub classes:  Vec<SwiftClass>,    // top-level class declarations
    pub funcs:    Vec<SwiftFunc>,     // top-level free functions
}
```

**Omissions by design:**
- `protocol`, `actor`, `typealias`, `extension` declarations are **not
  captured** in v1.0.0. They produce `Tier3` / `ShellOnly` classifier
  outputs and are passed through unmodified. A future SPEC-009 minor
  version will add them when the classifier requires them.
- Import statements are not captured. Sarah does not need to replicate
  Swift imports in Rust output.
- Top-level expressions and statements (script-mode Swift) are not
  captured. Sarah targets module-style Swift only.

### 2.2 `SwiftStruct`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwiftStruct {
    pub name:    String,           // declaration name, e.g. "Point"
    pub fields:  Vec<SwiftField>,  // stored properties (var/let)
    pub methods: Vec<SwiftFunc>,   // instance and static methods
    pub line:    usize,            // 1-based source line of `struct` keyword
}
```

### 2.3 `SwiftEnum`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwiftEnum {
    pub name:  String,              // declaration name
    pub cases: Vec<SwiftEnumCase>,  // all cases, including associated-value cases
    pub line:  usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwiftEnumCase {
    pub name:             String,        // case name
    pub associated_types: Vec<String>,   // associated value Swift type strings
                                         // empty for simple cases
}
```

### 2.4 `SwiftClass`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwiftClass {
    pub name:       String,
    pub superclass: Option<String>,  // first type in `: Superclass, Protocol` list
                                     // None if no inheritance clause
    pub fields:     Vec<SwiftField>,
    pub methods:    Vec<SwiftFunc>,
    pub has_deinit: bool,            // true if a `deinit` block is present
    pub line:       usize,
}
```

**Note on `superclass`:** Sarah's parser captures only the first name
after `:`. Protocol conformances beyond the first are discarded in
v1.0.0. The classifier uses `superclass.is_some()` to gate the
`CLASS-SUBCLASS` diagnostic (SPEC-003). This is sufficient for Tier 2
generation.

### 2.5 `SwiftFunc`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwiftFunc {
    pub name:        String,
    pub params:      Vec<SwiftParam>,
    pub return_type: String,    // Swift return type as a raw string, e.g. "[User]?"
                                // "Void" when absent
    pub is_async:    bool,
    pub is_throws:   bool,
    pub is_static:   bool,
    pub line:        usize,
}
```

### 2.6 `SwiftParam`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwiftParam {
    pub label:      Option<String>,  // external argument label; None for `_`
    pub name:       String,          // internal parameter name
    pub swift_type: String,          // base Swift type string, without `?`
    pub optional:   bool,            // true if the Swift type is T?
    pub has_default: bool,           // true if a default value expression exists
}
```

### 2.7 `SwiftField`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwiftField {
    pub name:       String,
    pub swift_type: String,   // base Swift type string, without `?`
    pub optional:   bool,     // true if the Swift type is T?
    pub mutable:    bool,     // true for `var`, false for `let`
}
```

---

## 3. Type Invariants

These invariants must hold for every `SwiftFile` value produced by either
parser backend (`TreeSitter` or `Regex`). The round-trip test suite
(`transpiler/tests/round_trip.rs`) is the enforcement mechanism.

1. **`swift_type` never contains a trailing `?`.** Optionality is
   factored out into the `optional: bool` field on `SwiftField` and
   `SwiftParam`. Consumers must reconstruct the full Swift type string
   as `format!("{}{}", swift_type, if optional { "?" } else { "" })`
   when needed for diagnostic messages.

2. **`return_type` is `"Void"` when the function has no return type.**
   It is never an empty string.

3. **`line` is 1-based.** The first line of the file is line 1, not 0.
   Consumers must not subtract 1 before emitting diagnostics.

4. **`name` fields are never empty strings.** The parser emits
   `"Unknown"` as a fallback if the name cannot be extracted. An empty
   `name` is a parser bug.

5. **`SwiftFile` fields are ordered by source position.** `structs`,
   `enums`, `classes`, and `funcs` are appended in the order they appear
   in the source file. Consumers that produce deterministic output (e.g.
   `shell_gen.rs`) may rely on this ordering.

6. **Both parser backends must produce structurally identical output**
   for the same valid Swift input. The `--parser regex` and
   `--parser treesitter` paths are tested for equivalence in
   `round_trip.rs`. Any divergence is a bug in whichever backend
   produces the non-canonical result.

---

## 4. Serialisation Contract

`SwiftFile` and all its constituent types derive `serde::Serialize` and
`serde::Deserialize`. The serialised form is the public API surface for
tooling built on top of Sarah (editor plugins, CI scripts, external
analysers).

### 4.1 JSON schema (normative summary)

```json
{
  "structs": [ SwiftStruct ],
  "enums":   [ SwiftEnum   ],
  "classes": [ SwiftClass  ],
  "funcs":   [ SwiftFunc   ]
}
```

All field names use **snake_case** (Serde default). No field is
`#[serde(skip)]`; all fields are always present in the serialised form,
including booleans and empty arrays.

### 4.2 Stability guarantee

- **Adding a new field** to any IR type is a **minor version bump** on
  this spec (e.g. 1.0.0 → 1.1.0). The new field must carry a
  `#[serde(default)]` attribute so existing serialised IR remains
  deserializable.
- **Removing or renaming a field** is a **major version bump** (e.g.
  1.0.0 → 2.0.0) and requires a migration note in this document and
  a CHANGELOG entry.
- **Changing a field's type** (e.g. `line: usize` → `line: u32`) is a
  **major version bump**.

The spec version is recorded in the header of this document. It is the
responsibility of the PR author to bump the version when modifying any
IR type.

---

## 5. Consumer Responsibilities

Every module that reads `SwiftFile` is a **consumer**. Current consumers:

| Consumer | Module | What it reads |
|----------|--------|---------------|
| Classifier | `classify.rs` | All fields; emits `TierResult` per declaration |
| Core codegen | `codegen.rs` | `structs`, `enums`, `classes`, `funcs`; emits Rust Core source |
| Shell generator | `shell_gen.rs` | `structs`, `enums`, `classes`, `funcs`; emits Swift Shell source |
| CLI JSON dump | `main.rs` (`--emit ir`) | Full `SwiftFile`; serialises to stdout |

**Consumer rules:**

1. Consumers must not assume undocumented fields exist. Use only the
   fields listed in §2.
2. Consumers must handle `SwiftField::optional` and `SwiftParam::optional`
   correctly — never assume a field is non-optional unless `optional ==
   false`.
3. Consumers must treat `return_type == "Void"` and `return_type == "()"`
   as equivalent (both are produced by different parse paths and must
   map to Rust `()`).
4. Consumers must not mutate `SwiftFile` values. The IR is immutable
   after construction. Transformations produce new values.

---

## 6. Adding a Field — Checklist

When a PR adds, removes, or renames a field on any IR type:

- [ ] Update the Rust struct in `transpiler/src/parser.rs`.
- [ ] Update the canonical definition in §2 of this document.
- [ ] Bump the **Status / Version** in this document's header.
- [ ] Add `#[serde(default)]` if the field is new (minor bump).
- [ ] Update all consumers listed in §5 that reference the changed field.
- [ ] Add or update a test in `transpiler/tests/round_trip.rs` that
      exercises the new field through both parser backends.
- [ ] Update `CHANGELOG.md` (once it exists) with the version bump and
      migration note if required.

---

## 7. Known Gaps (v1.0.0)

The following Swift constructs are not yet captured in `SwiftFile` and
will be added in future minor versions as the classifier requires them:

| Construct | Missing from | Planned spec version |
|-----------|-------------|----------------------|
| `protocol` declarations | `SwiftFile.protocols` | 1.1.0 |
| `extension` declarations | `SwiftFile.extensions` | 1.1.0 |
| `actor` declarations | `SwiftFile.actors` | 1.2.0 |
| `typealias` | `SwiftFile.typealiases` | 1.2.0 |
| Protocol conformance list on classes/structs | `SwiftClass.conformances`, `SwiftStruct.conformances` | 1.1.0 |
| Generic type parameters on declarations | `SwiftStruct.type_params`, `SwiftFunc.type_params` | 1.2.0 |
| Attributes (`@available`, `@discardableResult`, `@cancellable`) | `SwiftFunc.attributes` | 1.1.0 |

When any of these are implemented, the PR must follow the checklist in
§6 and update this table.

---

*This document is part of the Sarah DDD specification set. For the Swift→Rust
type mapping applied to `swift_type` strings in the IR, see SPEC-002.
For the classifier that consumes `SwiftFile` to assign tiers, see SPEC-001.
For the async shell generator that emits wrappers from `SwiftFunc.is_async`,
see SPEC-006.*
