//! Charli3 pull-oracle aggregation trigger.
//!
//! Owns: the imperative shell that spawns the Charli3 Python pull-oracle
//! client to trigger an on-chain aggregation. Aggregation is a
//! prerequisite to fetching a fresh `AggState` datum — the Python
//! client collects signed feeds from each oracle node, builds the
//! aggregation transaction, gathers signatures, and submits to Cardano
//! Preprod.
//!
//! Does NOT own:
//!
//! - **Parsing AggState bytes.** That lives in [`crate::charli3`] and
//!   consumes whatever CBOR the freshly aggregated UTxO carries.
//! - **Fetching the datum from Kupo.** Separate shell step, arriving
//!   in a later adapter module.
//! - **Authorizing anything.** The aggregation trigger only refreshes
//!   the on-chain feed; it carries no authority of its own over
//!   disbursement decisions.
//!
//! ## CLI contract
//!
//! This module wraps a single Charli3 CLI invocation:
//!
//! ```text
//! charli3 aggregate --config <yaml> [--auto-submit] [--feed-data <path>]
//! ```
//!
//! Flag semantics match the authoritative client README at
//! [charli3-pull-oracle-client](https://github.com/Charli3-Official/charli3-pull-oracle-client):
//!
//! - `--config` — YAML file describing the feed, oracle address, oracle
//!   nodes, and wallet mnemonic. Authoritative sample configs for
//!   Preprod ship in
//!   [hackathon-resources/configs](https://github.com/Charli3-Official/hackathon-resources/tree/main/configs).
//! - `--auto-submit` — skip the confirmation prompt and submit the
//!   aggregation transaction automatically. MVP always sets this.
//! - `--feed-data` — use a previously-saved feed JSON instead of
//!   hitting the oracle nodes. Useful for demo dry-runs that avoid
//!   repeatedly polling live nodes; `None` in production.
//!
//! ## Wallet handling
//!
//! The Charli3 client reads the signing-wallet mnemonic from the
//! YAML's `wallet.mnemonic` field. The sample Preprod config uses
//! shell-style variable expansion:
//!
//! ```yaml
//! wallet:
//!   mnemonic: "$WALLET_MNEMONIC"
//! ```
//!
//! Operators set `WALLET_MNEMONIC` in the environment before invoking
//! the trigger; the mnemonic never lands in a committed file. This
//! module inherits the parent process's environment by default, which
//! is the intended path — callers do not need to thread the mnemonic
//! through our Rust code explicitly. If a caller wants to scope the
//! env to the spawn, construct the [`std::process::Command`] directly
//! and wrap it; this module's convenience wrapper is environment-
//! transparent.
//!
//! ## Return shape
//!
//! On zero exit, the caller receives the raw stdout bytes verbatim —
//! log them for operational visibility, do not interpret them
//! semantically. A successful aggregation is visible on-chain a few
//! blocks later when the `AggState` UTxO at the oracle address carries
//! the refreshed datum; that observation is the next step (Kupo
//! fetch), not this module's responsibility.
//!
//! On non-zero exit, stdout and stderr are surfaced verbatim so the
//! caller can diagnose (rate-limit, insufficient funds, node
//! unreachable, etc.) without this module trying to interpret them.

use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Configuration for a single `charli3 aggregate` invocation.
///
/// Fields are `pub` so integration tests and operator tooling can
/// assemble a config directly. Every field reaches the CLI verbatim —
/// no hidden derivations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AggregateConfig {
    /// Path to the Charli3 YAML config file (e.g.
    /// `deploy/preprod/ada-usd-preprod.example.yml`).
    pub config_file: PathBuf,
    /// When true, passes `--auto-submit` so the client submits the
    /// aggregation transaction without interactive confirmation. MVP
    /// production use always sets this.
    pub auto_submit: bool,
    /// Optional path to a previously-saved feed JSON (produced by
    /// `charli3 feeds --output …`). When `Some(path)`, passed as
    /// `--feed-data <path>`; the client skips polling the live
    /// oracle nodes and uses the saved data instead. Primarily
    /// useful for demo dry-runs.
    pub feed_data: Option<PathBuf>,
}

impl AggregateConfig {
    /// Build an MVP config: the config file path plus
    /// `auto_submit = true` and no saved feed data.
    #[must_use]
    pub fn new_mvp(config_file: PathBuf) -> Self {
        Self {
            config_file,
            auto_submit: true,
            feed_data: None,
        }
    }
}

/// Deterministically build the `charli3 aggregate` argument vector.
///
/// Pure: no I/O, no environment reads, no global state. Same config
/// in, same `Vec<String>` out, always — the golden fixture
/// `fixtures/charli3/sample_aggregate_cli_args.golden.txt` pins this
/// output for a representative config so argument-shape drift fails
/// CI rather than shipping silently.
///
/// Flag order is fixed:
///
/// 1. `aggregate` (subcommand)
/// 2. `--config <path>`
/// 3. `--auto-submit` (if enabled)
/// 4. `--feed-data <path>` (if set)
#[must_use]
pub fn build_aggregate_args(config: &AggregateConfig) -> Vec<String> {
    let mut args = Vec::with_capacity(6);
    args.push("aggregate".to_string());
    args.push("--config".to_string());
    args.push(config.config_file.to_string_lossy().into_owned());
    if config.auto_submit {
        args.push("--auto-submit".to_string());
    }
    if let Some(path) = &config.feed_data {
        args.push("--feed-data".to_string());
        args.push(path.to_string_lossy().into_owned());
    }
    args
}

/// Error returned by [`aggregate`].
#[derive(Debug)]
pub enum AggregateError {
    /// Spawning the `charli3` binary failed (not found on PATH,
    /// permission denied, etc.).
    Spawn(io::Error),
    /// The CLI process ran but exited non-zero. `stdout` and `stderr`
    /// are surfaced verbatim so the caller can diagnose without this
    /// module interpreting them.
    NonZeroExit {
        exit_code: Option<i32>,
        stdout: Vec<u8>,
        stderr: Vec<u8>,
    },
}

/// Raw stdout bytes from a successful `charli3 aggregate` invocation.
///
/// Returned verbatim; callers log for operational visibility but do
/// not interpret semantically. A successful aggregation's effect is
/// observed on-chain via Kupo, not parsed from this stdout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawAggregateStdout {
    pub bytes: Vec<u8>,
}

/// Spawn `charli3 aggregate` with the arguments derived from `config`
/// and wait for it to exit.
///
/// `binary` is the path to the `charli3` binary. Passed explicitly so
/// tests can point at a fixture or non-existent path and production
/// can resolve against PATH or an absolute path.
///
/// Environment is inherited from the parent process. In particular,
/// any `WALLET_MNEMONIC` (or other env vars referenced by the YAML
/// config via `$VAR` expansion) must be set by the caller before
/// invocation.
pub fn aggregate(
    binary: &Path,
    config: &AggregateConfig,
) -> Result<RawAggregateStdout, AggregateError> {
    let args = build_aggregate_args(config);
    let output = Command::new(binary)
        .args(&args)
        .output()
        .map_err(AggregateError::Spawn)?;
    if !output.status.success() {
        return Err(AggregateError::NonZeroExit {
            exit_code: output.status.code(),
            stdout: output.stdout,
            stderr: output.stderr,
        });
    }
    Ok(RawAggregateStdout {
        bytes: output.stdout,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn sample_config() -> AggregateConfig {
        AggregateConfig::new_mvp(PathBuf::from(
            "/etc/oracleguard/deploy/preprod/ada-usd-preprod.yml",
        ))
    }

    // ---- AggregateConfig defaults ----

    #[test]
    fn new_mvp_enables_auto_submit() {
        let c = sample_config();
        assert!(c.auto_submit);
    }

    #[test]
    fn new_mvp_has_no_feed_data() {
        let c = sample_config();
        assert!(c.feed_data.is_none());
    }

    #[test]
    fn new_mvp_carries_config_path_verbatim() {
        let c = sample_config();
        assert_eq!(
            c.config_file,
            PathBuf::from("/etc/oracleguard/deploy/preprod/ada-usd-preprod.yml")
        );
    }

    // ---- build_aggregate_args — determinism and shape ----

    #[test]
    fn build_aggregate_args_is_deterministic() {
        let c = sample_config();
        let a = build_aggregate_args(&c);
        let b = build_aggregate_args(&c);
        assert_eq!(a, b);
    }

    #[test]
    fn build_aggregate_args_emits_expected_flag_order() {
        let args = build_aggregate_args(&sample_config());
        assert_eq!(args[0], "aggregate");
        assert_eq!(args[1], "--config");
        // args[2] is the config path value.
        assert_eq!(args[3], "--auto-submit");
    }

    #[test]
    fn build_aggregate_args_emits_config_path_after_flag() {
        let args = build_aggregate_args(&sample_config());
        assert_eq!(
            args[2],
            "/etc/oracleguard/deploy/preprod/ada-usd-preprod.yml"
        );
    }

    // ---- build_aggregate_args — flag toggling ----

    #[test]
    fn build_aggregate_args_omits_auto_submit_when_false() {
        let mut c = sample_config();
        c.auto_submit = false;
        let args = build_aggregate_args(&c);
        assert!(!args.iter().any(|a| a == "--auto-submit"));
    }

    #[test]
    fn build_aggregate_args_includes_feed_data_when_set() {
        let mut c = sample_config();
        c.feed_data = Some(PathBuf::from("/tmp/feeds.json"));
        let args = build_aggregate_args(&c);
        let flag_pos = args
            .iter()
            .position(|a| a == "--feed-data")
            .expect("--feed-data must appear when feed_data is Some");
        assert_eq!(args[flag_pos + 1], "/tmp/feeds.json");
    }

    #[test]
    fn build_aggregate_args_omits_feed_data_when_none() {
        let c = sample_config();
        let args = build_aggregate_args(&c);
        assert!(!args.iter().any(|a| a == "--feed-data"));
    }

    #[test]
    fn build_aggregate_args_feed_data_appears_after_auto_submit() {
        // Flag order: --config <path>, --auto-submit, --feed-data <path>.
        // This test pins the ordering so a later refactor that
        // reorders the flags fails CI rather than silently changing
        // the CLI invocation shape.
        let mut c = sample_config();
        c.feed_data = Some(PathBuf::from("/tmp/feeds.json"));
        let args = build_aggregate_args(&c);
        let auto_pos = args.iter().position(|a| a == "--auto-submit").unwrap();
        let feed_pos = args.iter().position(|a| a == "--feed-data").unwrap();
        assert!(
            feed_pos > auto_pos,
            "--feed-data must appear after --auto-submit"
        );
    }

    // ---- build_aggregate_args — per-field isolation ----

    #[test]
    fn build_aggregate_args_config_path_change_isolates_to_path_position() {
        let a = build_aggregate_args(&sample_config());
        let mut c2 = sample_config();
        c2.config_file = PathBuf::from("/etc/oracleguard/deploy/preprod/btc-usd-preprod.yml");
        let b = build_aggregate_args(&c2);
        assert_eq!(a.len(), b.len());
        for (i, (x, y)) in a.iter().zip(b.iter()).enumerate() {
            if i == 2 {
                assert_ne!(x, y);
            } else {
                assert_eq!(
                    x, y,
                    "arg {i} must not change when only config path changes"
                );
            }
        }
    }

    // ---- Golden fixture ----

    #[test]
    fn build_aggregate_args_matches_golden_fixture() {
        const GOLDEN: &str =
            include_str!("../../../fixtures/charli3/sample_aggregate_cli_args.golden.txt");
        let args = build_aggregate_args(&sample_config());
        let rendered = args.join("\n");
        assert_eq!(rendered.trim_end(), GOLDEN.trim_end());
    }

    // ---- aggregate() spawn-failure surfacing ----

    #[test]
    fn aggregate_surfaces_spawn_error_for_missing_binary() {
        let bin = Path::new("/nonexistent/charli3/binary/does_not_exist");
        let err = aggregate(bin, &sample_config()).expect_err("missing binary must error");
        match err {
            AggregateError::Spawn(_) => {}
            AggregateError::NonZeroExit { .. } => {
                panic!("expected Spawn, got NonZeroExit")
            }
        }
    }
}
