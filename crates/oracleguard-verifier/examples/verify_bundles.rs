//! Per-bundle verifier walkthrough for the demo.
//!
//! Loads each recorded evidence bundle in order, prints what it
//! encodes (intent_id, allocation basis, authorization verdict,
//! execution outcome), runs the offline verifier over it, and
//! prints CLEAN / FAILED with any findings.
//!
//! Exits 0 when every bundle verifies CLEAN, 1 otherwise.
//!
//! Run:
//!     cargo run -p oracleguard-verifier --example verify_bundles

use std::path::Path;

use oracleguard_schemas::evidence::{
    AuthorizationSnapshotV1, DisbursementEvidenceV1, ExecutionOutcomeV1, FulfillmentRejectionKindV1,
};
use oracleguard_schemas::gate::AuthorizationGate;
use oracleguard_schemas::reason::DisbursementReasonCode;
use oracleguard_verifier::{load_bundle, verify_evidence};

const BUNDLES: &[&str] = &[
    "allow_700_ada_bundle.postcard",
    "deny_900_ada_bundle.postcard",
    "reject_non_ada_bundle.postcard",
    "reject_pending_bundle.postcard",
];

fn main() {
    let mut clean = 0;
    for (i, name) in BUNDLES.iter().enumerate() {
        let path = format!("fixtures/evidence/{name}");
        println!("── Bundle {}/{}  {}", i + 1, BUNDLES.len(), name);
        match load_bundle(Path::new(&path)) {
            Ok(evidence) => {
                describe(&evidence);
                let report = verify_evidence(&evidence);
                if report.is_ok() {
                    println!("   verdict                : CLEAN");
                    clean += 1;
                } else {
                    println!("   verdict                : FAILED");
                    for f in &report.findings {
                        println!("     finding              : {f:?}");
                    }
                }
            }
            Err(e) => {
                println!("   load error             : {e:?}");
            }
        }
        println!();
    }
    println!("summary: {}/{} bundles CLEAN", clean, BUNDLES.len());
    if clean != BUNDLES.len() {
        std::process::exit(1);
    }
}

fn describe(e: &DisbursementEvidenceV1) {
    println!(
        "   bundle version         : v{}  ({} bytes of canonical postcard)",
        e.evidence_version,
        approx_bytes(e),
    );
    println!("   intent_id              : {}", hex32(&e.intent_id));
    println!(
        "   requested amount       : {} lovelace  ({} ADA)",
        e.intent.requested_amount_lovelace,
        e.intent.requested_amount_lovelace / 1_000_000
    );
    println!(
        "   allocation basis       : {} lovelace  ({} ADA)",
        e.allocation_basis_lovelace,
        e.allocation_basis_lovelace / 1_000_000
    );
    println!(
        "   oracle price           : {} microusd  (~${}.{:03})",
        e.intent.oracle_fact.price_microusd,
        e.intent.oracle_fact.price_microusd / 1_000_000,
        (e.intent.oracle_fact.price_microusd % 1_000_000) / 1_000,
    );
    println!("   committed height       : {}", e.committed_height);
    println!(
        "   authorization          : {}",
        describe_auth(&e.authorization)
    );
    println!(
        "   execution              : {}",
        describe_exec(&e.execution)
    );
    println!(
        "   checks                 : intent_id recompute + gate/reason invariants + replay evaluator"
    );
}

fn describe_auth(a: &AuthorizationSnapshotV1) -> String {
    match a {
        AuthorizationSnapshotV1::Authorized { effect } => format!(
            "Authorized  ({} ADA authorized, band = {} bps)",
            effect.authorized_amount_lovelace / 1_000_000,
            effect.release_cap_basis_points,
        ),
        AuthorizationSnapshotV1::Denied { reason, gate } => format!(
            "Denied      ({} at {} gate)",
            reason_name(*reason),
            gate_name(*gate),
        ),
    }
}

fn describe_exec(x: &ExecutionOutcomeV1) -> String {
    match x {
        ExecutionOutcomeV1::Settled { tx_hash } => {
            format!("Settled     (tx = {}…)", short_hex(tx_hash, 8))
        }
        ExecutionOutcomeV1::DeniedUpstream { reason, gate } => format!(
            "DeniedUpstream ({} at {} gate — no tx)",
            reason_name(*reason),
            gate_name(*gate),
        ),
        ExecutionOutcomeV1::RejectedAtFulfillment { kind } => {
            format!("RejectedAtFulfillment ({}) — no tx", rejection_name(*kind))
        }
    }
}

fn reason_name(r: DisbursementReasonCode) -> &'static str {
    match r {
        DisbursementReasonCode::PolicyNotFound => "PolicyNotFound",
        DisbursementReasonCode::AllocationNotFound => "AllocationNotFound",
        DisbursementReasonCode::OracleStale => "OracleStale",
        DisbursementReasonCode::OraclePriceZero => "OraclePriceZero",
        DisbursementReasonCode::AmountZero => "AmountZero",
        DisbursementReasonCode::ReleaseCapExceeded => "ReleaseCapExceeded",
        DisbursementReasonCode::SubjectNotAuthorized => "SubjectNotAuthorized",
        DisbursementReasonCode::AssetMismatch => "AssetMismatch",
    }
}

fn gate_name(g: AuthorizationGate) -> &'static str {
    match g {
        AuthorizationGate::Anchor => "Anchor",
        AuthorizationGate::Registry => "Registry",
        AuthorizationGate::Grant => "Grant",
    }
}

fn rejection_name(k: FulfillmentRejectionKindV1) -> &'static str {
    match k {
        FulfillmentRejectionKindV1::NonAdaAsset => "NonAdaAsset",
        FulfillmentRejectionKindV1::ReceiptNotCommitted => "ReceiptNotCommitted",
    }
}

fn approx_bytes(e: &DisbursementEvidenceV1) -> usize {
    oracleguard_schemas::evidence::encode_evidence(e)
        .map(|v| v.len())
        .unwrap_or(0)
}

fn hex32(bytes: &[u8; 32]) -> String {
    let mut s = String::with_capacity(64);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

fn short_hex(bytes: &[u8; 32], n: usize) -> String {
    let take = n.min(bytes.len());
    let mut s = String::with_capacity(take * 2);
    for b in &bytes[..take] {
        s.push_str(&format!("{b:02x}"));
    }
    s
}
