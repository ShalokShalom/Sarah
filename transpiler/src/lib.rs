//! sarah-cli-lib — public library surface for integration tests
//!
//! Re-exports all pipeline modules so that `tests/` can import them
//! via `use sarah_cli_lib::...`.

pub mod classify;
pub mod codegen;
pub mod diagnostics;
pub mod drop_gen;
pub mod parser;
pub mod shell_gen;
pub mod types;
