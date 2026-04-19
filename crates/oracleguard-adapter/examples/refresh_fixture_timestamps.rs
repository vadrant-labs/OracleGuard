//! Rewrite an intent fixture with fresh oracle_provenance timestamps.
//!
//! The Ziranity WCA_01 change made wall-clock a consensus-covered
//! quantity, so the grant gate's freshness check runs against the
//! leader's real clock at proposal time. The checked-in integration
//! fixtures have timestamps from the original capture date, which go
//! stale relative to any real run after the 300 s window closes.
//!
//! This helper reads an intent postcard, mutates only
//! `oracle_provenance.timestamp_unix` and `oracle_provenance.expiry_unix`
//! to current wall-clock (+250 s window), and writes the result back.
//! Every other field — including oracle_fact, policy_ref, and thus
//! intent_id (which excludes oracle_provenance by canonical projection)
//! — is preserved byte-for-byte. The corresponding expected-result
//! fixture therefore remains valid: `AuthorizedEffectV1` does not carry
//! provenance, and the denial-reason-code math does not depend on it.
//!
//! Usage:
//!     refresh_fixture_timestamps <input.postcard> [output.postcard]
//!
//! If `output` is omitted the file is rewritten in place.

use std::time::{SystemTime, UNIX_EPOCH};

use oracleguard_schemas::encoding::{decode_intent, encode_intent};

const FRESH_WINDOW_MS: u64 = 250_000; // 250 s, well inside Charli3's 300 s window

fn main() {
    let mut args = std::env::args().skip(1);
    let input = match args.next() {
        Some(p) => p,
        None => {
            eprintln!("usage: refresh_fixture_timestamps <input> [output]");
            std::process::exit(2);
        }
    };
    let output = args.next().unwrap_or_else(|| input.clone());

    let bytes = match std::fs::read(&input) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("failed to read {input}: {e}");
            std::process::exit(1);
        }
    };
    let mut intent = match decode_intent(&bytes) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("failed to decode intent at {input}: {e:?}");
            std::process::exit(1);
        }
    };

    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    intent.oracle_provenance.timestamp_unix = now_ms;
    intent.oracle_provenance.expiry_unix = now_ms + FRESH_WINDOW_MS;

    let out_bytes = match encode_intent(&intent) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("failed to encode refreshed intent: {e:?}");
            std::process::exit(1);
        }
    };
    if let Err(e) = std::fs::write(&output, &out_bytes) {
        eprintln!("failed to write {output}: {e}");
        std::process::exit(1);
    }

    println!(
        "refreshed {} → {}  now_ms={}  expiry_ms={}  ({} bytes)",
        input,
        output,
        now_ms,
        now_ms + FRESH_WINDOW_MS,
        out_bytes.len()
    );
}
