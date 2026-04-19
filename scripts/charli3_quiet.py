#!/usr/bin/env python3
"""Run the Charli3 ODV client with pycardano's WARNING-level state
dump suppressed.

pycardano's TransactionBuilder emits a ~200-line WARNING containing
the entire builder state (UTxO maps, redeemers, collateral, metadata,
etc.) on every build(). Useful for debugging, terrible for a live
demo — it drowns out the actual Charli3 progress output and the
error message on failure.

This wrapper sets the PyCardano logger level to ERROR before
importing the client, then delegates argv to charli3_odv_client.cli.main
exactly as the venv's bin/charli3 does. No behavioural change beyond
logger level.

Invoke exactly like you would `.venv/bin/charli3`:

    .venv/bin/python scripts/charli3_quiet.py aggregate --config ...
"""

import logging
import sys

logging.getLogger("PyCardano").setLevel(logging.ERROR)

from charli3_odv_client.cli import main  # noqa: E402  (import after logging config)

if __name__ == "__main__":
    sys.argv[0] = sys.argv[0].removesuffix(".exe")
    sys.exit(main())
