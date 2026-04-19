#!/usr/bin/env python3
"""Print the original and rotated policy_ref side-by-side.

Demonstrates the governance claim: changing a rule in the policy
JSON changes the 32-byte policy_ref that every intent references.
Original values come from fixtures/policy_v1.json; the rotated
version replaces release_cap_basis_points with --cap-bps (default
10000 = 100%).

Canonicalization matches oracleguard_schemas::policy::
canonicalize_policy_json (sort object keys recursively, compact,
no insignificant whitespace).
"""

import argparse
import hashlib
import json
import pathlib


def canonical(policy: dict) -> bytes:
    return json.dumps(policy, sort_keys=True, separators=(",", ":")).encode()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--cap-bps",
        type=int,
        default=10_000,
        help="release_cap_basis_points for the rotated policy (default: 10000 = 100%)",
    )
    args = parser.parse_args()

    repo_root = pathlib.Path(__file__).resolve().parent.parent
    original_json = (repo_root / "fixtures/policy_v1.json").read_bytes()
    original = json.loads(original_json)

    rotated = dict(original)
    rotated["release_cap_basis_points"] = args.cap_bps

    orig_canon = canonical(original)
    new_canon = canonical(rotated)

    orig_ref = hashlib.sha256(orig_canon).hexdigest()
    new_ref = hashlib.sha256(new_canon).hexdigest()

    print(f"ORIGINAL policy   (release_cap_basis_points = {original['release_cap_basis_points']})")
    print(f"  canonical bytes : {len(orig_canon)}")
    print(f"  policy_ref      : {orig_ref}")
    print()
    print(f"ROTATED policy    (release_cap_basis_points = {rotated['release_cap_basis_points']})")
    print(f"  canonical bytes : {len(new_canon)}")
    print(f"  policy_ref      : {new_ref}")
    print()
    print(f"  changed         : {'yes' if orig_ref != new_ref else 'no'}")


if __name__ == "__main__":
    main()
