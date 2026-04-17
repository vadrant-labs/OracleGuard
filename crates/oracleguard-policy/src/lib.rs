//! OracleGuard release-cap policy evaluation.
//!
//! This crate owns the pure deterministic evaluator over canonical
//! semantic inputs from `oracleguard-schemas`. It must not perform I/O,
//! must not use floating-point arithmetic in authoritative paths, and
//! must not depend on the adapter or verifier crates.
//!
//! The public surface is:
//! - [`evaluate::evaluate_disbursement`] — the decision entry point
//! - [`error::EvaluationResult`] — the typed, closed result surface
//! - [`math`] — integer-only helpers the evaluator composes
//!
//! See `docs/evaluator-contract.md` for the full contract and
//! `docs/evaluator-fixtures.md` for the golden allow/deny fixtures.

#![forbid(unsafe_code)]
#![deny(clippy::float_arithmetic)]
#![deny(clippy::panic)]
#![deny(clippy::todo, clippy::unimplemented)]
#![deny(clippy::unwrap_used, clippy::expect_used)]
#![deny(clippy::print_stdout, clippy::print_stderr)]
#![deny(clippy::dbg_macro)]

pub mod authorize;
pub mod error;
pub mod evaluate;
pub mod math;

pub use error::EvaluationResult;
pub use evaluate::evaluate_disbursement;
