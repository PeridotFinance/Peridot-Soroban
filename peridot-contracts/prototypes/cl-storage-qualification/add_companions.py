#!/usr/bin/env python3
"""Copy a pinned build and capture live/data/cold transactions from one prestate.

Each is a separate complete official-host invocation with the same limits, not
stages of one operation. Fixture construction and SDK execution are shared only
to avoid repeating the large historical setup. All three raw rows are retained.
"""
import argparse
import hashlib
import json
from pathlib import Path
import shutil

p=argparse.ArgumentParser(description=__doc__)
p.add_argument('--build',type=Path,required=True)
p.add_argument('--output',type=Path,required=True)
a=p.parse_args()
assert not a.output.exists()
m=json.loads((a.build/'build-manifest.json').read_text())
for name,h in m['harness_generated_sources'].items():
    assert hashlib.sha256((a.build/name).read_bytes()).hexdigest()==h
for name,pin in m['wasm'].items():
    assert hashlib.sha256((a.build/'artifacts'/name).read_bytes()).hexdigest()==pin['sha256']
a.output.mkdir(parents=True)
for name in ['artifacts','harness']:
    shutil.copytree(a.build/name,a.output/name)
f=a.output/'harness/src/transaction_evidence.rs'
s=f.read_text()
s=s.replace('wrong_args, None)?;', 'wrong_args, None, false)?;')
s=s.replace('wrong_args, Some(&role))?;', 'wrong_args, Some(&role), std::env::var("CL_STORAGE_ARCHIVE_CODE_INSTANCE").as_deref() == Ok("1"))?;')
s=s.replace('    archive_role: Option<&str>,\n', '    archive_role: Option<&str>,\n    archive_code: bool,\n')
s=s.replace('        let archive_code = std::env::var("CL_STORAGE_ARCHIVE_CODE_INSTANCE").as_deref() == Ok("1");\n','')
anchor='        return Ok(archived);'
assert s.count(anchor)==1
s=s.replace(anchor, '''        // Independent fully metered comparisons; never a prerequisite warmup.
        let mut data = simulate_one(e, to, name, args, external, enforce_empty_auth, wrong_args, Some(&role), false)
            .unwrap_or_else(|error| json!({"success": false, "error": error}));
        data["archive"]["baseline_return_value_xdr"] = baseline["return_value_xdr"].clone();
        data["archive"]["return_matches_baseline"] = json!(data["return_value_xdr"] == baseline["return_value_xdr"]);
        data["archive"]["contract_changes_match_baseline"] = json!(data["post_state_data_sha256"] == baseline["post_state_data_sha256"]);
        // Raw sibling evidence, not reconstructed execution quantities.
        data["archive"]["baseline_post_state_data_sha256"] = baseline["post_state_data_sha256"].clone();
        data["archive"]["baseline_snapshot_sha256"] = baseline["snapshot_sha256"].clone();
        data["archive"]["baseline_fees"] = baseline["fees"].clone();
        archived["storage_live_transaction"] = baseline;
        archived["storage_data_transaction"] = data;
        return Ok(archived);''')
# A cold host setup/recording error must not suppress an independently completed
# live/data result. Missing cold quantities remain explicitly failed evidence.
s=s.replace('wrong_args, None, false)?;',
            'wrong_args, None, false).unwrap_or_else(|error| json!({"success": false, "error": error}));')
s=s.replace('std::env::var("CL_STORAGE_ARCHIVE_CODE_INSTANCE").as_deref() == Ok("1"))?;',
            'std::env::var("CL_STORAGE_ARCHIVE_CODE_INSTANCE").as_deref() == Ok("1")).unwrap_or_else(|error| json!({"success": false, "error": error}));')
f.write_text(s)
m['harness_generated_sources']={str(f.relative_to(a.output)):hashlib.sha256(f.read_bytes()).hexdigest() for f in sorted((a.output/'harness/src').glob('*.rs'))}
m['companion_capture']={'script_sha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),'parent_manifest_sha256':hashlib.sha256((a.build/'build-manifest.json').read_bytes()).hexdigest(),'each_independent_host_metered':True}
(a.output/'build-manifest.json').write_text(json.dumps(m,indent=2)+'\n')
print(a.output)
