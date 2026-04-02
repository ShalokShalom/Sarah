//! Drop / deinit emitter — Phase 2c
//!
//! When a Swift `class` contains a `deinit` block, the Rust translation
//! must implement the `Drop` trait on the inner struct to run cleanup
//! code when the `Arc` reference count reaches zero.
//!
//! This module analyses the parsed deinit body (from `parser::SwiftClass`)
//! and generates a best-effort `impl Drop` block. For patterns it cannot
//! translate it emits a diagnostic and a `todo!()` stub so the project
//! still compiles.
//!
//! # Rules (SPEC-003 §6 — Drop translation)
//!
//! | Swift deinit pattern | Rust output |
//! |---|---|
//! | `close(handle)` / `handle.close()` | `// close(self.handle)` stub + warn |
//! | `NotificationCenter.default.removeObserver(self)` | `// removeObserver stub` + T2-OBSERVER diagnostic |
//! | `delegate = nil` | `self.delegate = None;` |
//! | Empty deinit | `impl Drop { fn drop(&mut self) {} }` |
//! | Anything else | `todo!("deinit: <source>")` + T2-DEINIT diagnostic |

use crate::diagnostics::{Diagnostic, Severity};
use crate::parser::SwiftClass;

/// Emit a Rust `impl Drop` block for a Swift class with `deinit`.
///
/// Returns `(rust_source, diagnostics)`.
pub fn emit_drop(class: &SwiftClass, deinit_body: &[&str]) -> (String, Vec<Diagnostic>) {
    let mut diags: Vec<Diagnostic> = Vec::new();
    let mut body_lines: Vec<String> = Vec::new();

    if deinit_body.is_empty() {
        // Empty deinit — trivial Drop
        return (
            format!(
                "impl Drop for {}Inner {{\n    fn drop(&mut self) {{}}\n}}\n\n",
                class.name
            ),
            diags,
        );
    }

    for line in deinit_body {
        let t = line.trim();
        if t.is_empty() || t.starts_with("//") { continue; }

        if t.contains("NotificationCenter") {
            diags.push(Diagnostic {
                code:     "T2-OBSERVER".into(),
                severity: Severity::Warning,
                message:  format!(
                    "`{}` deinit removes NotificationCenter observer — stub generated; \
                     wire up manually after transpilation.",
                    class.name
                ),
                line:     class.line,
                file:     String::new(),
            });
            body_lines.push(format!("        // T2-OBSERVER: {t}"));
        } else if let Some(field) = extract_nil_assign(t) {
            // `field = nil` → `self.field = None;`
            body_lines.push(format!("        self.{field} = None;"));
        } else if looks_like_close(t) {
            diags.push(Diagnostic {
                code:     "T2-DEINIT-CLOSE".into(),
                severity: Severity::Warning,
                message:  format!(
                    "`{}` deinit calls close/free — verify the Rust resource handle is correct.",
                    class.name
                ),
                line:     class.line,
                file:     String::new(),
            });
            body_lines.push(format!("        // T2-DEINIT-CLOSE: {t}"));
        } else {
            diags.push(Diagnostic {
                code:     "T2-DEINIT".into(),
                severity: Severity::Warning,
                message:  format!(
                    "`{}` deinit contains untranslatable statement — `todo!()` stub generated.",
                    class.name
                ),
                line:     class.line,
                file:     String::new(),
            });
            body_lines.push(format!("        todo!(\"T2-DEINIT: {}\", /* {t} */)", class.name));
        }
    }

    let body = if body_lines.is_empty() {
        "        // (nothing to do)".to_owned()
    } else {
        body_lines.join("\n")
    };

    let rust = format!(
        "/// Drop impl generated from Swift `deinit` in `{}` (SPEC-003 §6)\n\
         impl Drop for {}Inner {{\n    fn drop(&mut self) {{\n{body}\n    }}\n}}\n\n",
        class.name, class.name
    );

    (rust, diags)
}

fn extract_nil_assign(line: &str) -> Option<&str> {
    let stripped = line.trim_end_matches(';').trim();
    if stripped.ends_with("= nil") {
        let field = stripped.trim_end_matches("= nil").trim();
        if !field.contains(' ') && !field.contains('.') {
            return Some(field);
        }
    }
    None
}

fn looks_like_close(line: &str) -> bool {
    let l = line.to_lowercase();
    l.contains("close(") || l.contains(".close()") ||
    l.contains("free(")  || l.contains("release(") ||
    l.contains("destroy(")
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::SwiftClass;

    fn class(name: &str) -> SwiftClass {
        SwiftClass {
            name:       name.to_owned(),
            superclass: None,
            fields:     vec![],
            methods:    vec![],
            has_deinit: true,
            line:       1,
        }
    }

    #[test]
    fn empty_deinit_generates_trivial_drop() {
        let (rust, diags) = emit_drop(&class("MyClass"), &[]);
        assert!(rust.contains("impl Drop for MyClassInner"));
        assert!(rust.contains("fn drop(&mut self) {}"));
        assert!(diags.is_empty());
    }

    #[test]
    fn nil_assign_translates_to_none() {
        let body = vec!["delegate = nil"];
        let (rust, diags) = emit_drop(&class("Session"), &body);
        assert!(rust.contains("self.delegate = None;"));
        assert!(diags.is_empty());
    }

    #[test]
    fn notification_center_emits_warning() {
        let body = vec!["NotificationCenter.default.removeObserver(self)"];
        let (rust, diags) = emit_drop(&class("Observer"), &body);
        assert!(diags.iter().any(|d| d.code == "T2-OBSERVER"));
        assert!(rust.contains("T2-OBSERVER:"));
    }

    #[test]
    fn close_call_emits_close_warning() {
        let body = vec!["handle.close()"];
        let (rust, diags) = emit_drop(&class("Resource"), &body);
        assert!(diags.iter().any(|d| d.code == "T2-DEINIT-CLOSE"));
    }

    #[test]
    fn unknown_statement_emits_todo() {
        let body = vec!["someComplexOperation()"];
        let (rust, diags) = emit_drop(&class("Fx"), &body);
        assert!(rust.contains("todo!"));
        assert!(diags.iter().any(|d| d.code == "T2-DEINIT"));
    }
}
