**OracleGuard** — policy-governed Cardano treasury disbursements with live oracle pricing and offline verification

A Cardano treasury releases funds only when an anchored, public policy allows it:
• live Charli3 ADA/USD price shapes the band-specific release cap
• deterministic 3-gate closure (anchor → registry → grant) runs inside a 4-node Ziranity BFT devnet
• on *Authorized*, a real Preprod Cardano transaction settles the exact authorized amount
• on *Denied*, no transaction exists by design
• every decision emits a canonical evidence bundle; any auditor replays it offline via `oracleguard-verifier` and reaches the same verdict with no trust in the network

🔗 GitHub: https://github.com/wdm33/OracleGuard
🎬 Demo video: <FILL IN>
⚙️ Stack: Rust (4 public crates), Charli3 ODV pull oracle, Ziranity BFT consensus, Cardano Preprod settlement via Ogmios + pycardano, offline verifier
