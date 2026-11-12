#!/usr/bin/env python3
"""Recreate exact code-only Core input from preserved unsigned raw evidence."""
import hashlib, json
from pathlib import Path
root = Path(__file__).resolve().parent
meta = json.loads((root / "receipt-code-input.json").read_text())
config = Path(meta["network_config"])
assert hashlib.sha256(config.read_bytes()).hexdigest() == meta["network_config_sha256"]
source = Path(meta["original_source"])
assert hashlib.sha256(source.read_bytes()).hexdigest() == meta["source_sha256"]
rows = [json.loads(line) for line in source.read_text().splitlines() if line.strip()]
row = meta["row"]
assert row in rows
settings = json.loads(config.read_text())["settings"]
assert len(settings) == 13
lines = [str(row["current_ledger"]), str(row["fees"]["total"]), row["unsigned_envelope_xdr"], str(len(settings))]
lines += [value["xdr"] for value in settings.values()]
lines.append(str(len(row["keys"])))
for key in row["keys"]:
    lines += [key["entry_xdr"], str(key["old_ttl"])]
expected = ("\n".join(lines) + "\n").encode()
assert expected == (root / "receipt-code-input.txt").read_bytes()
print(json.dumps({"input_sha256": hashlib.sha256(expected).hexdigest(), "bytes":len(expected), "matches_preserved_input":True}))
