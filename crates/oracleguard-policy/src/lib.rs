//! OracleGuard release-cap policy evaluation.
//!
//! This crate owns the pure deterministic evaluator over canonical semantic
//! inputs from `oracleguard-schemas`. It must not perform I/O, must not use
//! floating-point arithmetic in authoritative paths, and must not depend on
//! the adapter or verifier crates.
