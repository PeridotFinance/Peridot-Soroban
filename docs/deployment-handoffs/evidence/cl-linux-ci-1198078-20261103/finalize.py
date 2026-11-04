import hashlib, json, shutil
from pathlib import Path

root = Path(__file__).resolve().parent
dest = root / 'linux-1198078'
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
original = {name: sha(dest / name) for name in ['index.json', 'linux-ci-payload.tar.xz']}
summary = json.loads((root / 'verified-summary.json').read_text())
index = json.loads((dest / 'index.json').read_text())
support = ['verified-summary.json', 'capture-identity.json', 'github-metadata.json', 'checker-job.log', 'verify.py', 'finalize.py']
for name in support:
    assert not (dest / name).exists()
    shutil.copyfile(root / name, dest / name)
(dest / 'supporting-checksums.json').write_text(json.dumps({name: sha(dest / name) for name in support}, indent=2) + '\n')
lines = [
    '# Linux baseline CI evidence: 1198078', '',
    'Completed [CI run41512536485](https://github.com/PeridotFinance/Peridot-Soroban/actions/runs/41512536485) checked PR75 head `11980783cd0da692dee97394a21c949757a6e910` through merge commit `025782585be9abfe88722303058b460a19807604`. The overall run failed: checker job125508485816 passed42 tests; checkpoint job125508485751 failed its gate step and successfully uploaded its artifact. Stage1 remains incomplete, PR75 draft and release approval false.', '',
    'This workflow executes the baseline checkpoint runner. It does **not** execute the isolated split, atomic-qualification or subsequent storage candidates. These baseline results cannot approve those experiments.', '',
    '## Durable identities', '',
    '| Item | Identity |', '| --- | --- |',
    '| Artifact | `13176432381`, `cl-stage1-evidence-025782585be9abfe88722303058b460a19807604` |',
    '| GitHub expiry | `2026-11-16T20:27:22Z` |',
    '| Original ZIP | 200952624 bytes, 75 members |',
    '| Original ZIP SHA-256 | `ef03eac5e1f67f3fdb470f5e5b4366d8af06e51fb9773a1d5dd4ba89827660c7` |',
    f'| Durable TAR.XZ | {index["archive_bytes"]} bytes, {len(index["payload_members"])} members |',
    f'| Durable TAR.XZ SHA-256 | `{index["archive_sha256"]}` |',
    f'| Index SHA-256 | `{original["index.json"]}` |',
    f'| Captured configuration SHA-256 | `{summary["config_sha256"]}` |', '',
    '[index.json](index.json) records every member hash and length, both complete gate/error summaries, and all10 Linux WASM identities, including the separately built default V3 and historical receipt ABI fixture. [linux-ci-payload.tar.xz](linux-ci-payload.tar.xz) losslessly preserves all75 original ZIP members and22 pinned replay/proof source files. It is independent of older Linux archives and local candidate builds.', '',
    '## Gate results and completed tests', '',
    '| Profile | Invocation | Lifecycle | Transaction | Checkpoint | Missing flows | Lifecycle errors | Transaction errors |',
    '| --- | --- | --- | --- | --- | ---: | ---: | ---: |']
for profile in ['captured-network-diagnostic', 'internal-policy']:
    s = summary['profiles'][profile]['gate_summary']
    gates = ['PASS' if s['gates'][g] else 'FAIL' for g in ['invocation', 'lifecycle', 'transaction', 'checkpoint']]
    errors = [str(len(s[k])) for k in ['missing_required_flows', 'lifecycle_errors', 'transaction_errors']]
    lines.append('| `' + profile + '` | ' + ' | '.join(gates + errors) + ' |')
lines += ['', 'A zero missing-flow-label count does not make failed tests pass. The saved internal lifecycle and transaction errors remain failures; aggregate exit1 is not the verification criterion.', '',
    '| Suite | Captured profile | Internal profile |', '| --- | --- | --- |',
    '| Compiled baseline main | 36 passed, 0 failed | 4 passed, 32 failed |',
    '| Compiled archival reruns | 3 passed, 0 failed | 0 passed, 3 failed |',
    '| Compiled history archival reruns | 2 passed, 0 failed | 0 passed, 2 failed |',
    '| Runtime integrity | 1 passed, 0 failed | 1 passed, 0 failed |',
    '| Native funded, separate host25 evidence | 9 passed, 0 failed | 5 passed, 4 failed |', '',
    'Shared suites: checker42 passed; native reward tests53 passed; default margin regression110 passed and2 ignored. Archival reruns repeat selected tests; they are not five additional distinct main tests. Both profiles map46 proof labels. [verified-summary.json](verified-summary.json) preserves each named terminal outcome, full errors and measured maxima.', '',
    '## Measured scope', '',
    '| Profile | SDK rows | Unsigned records | SDK CPU max | SDK memory max | SDK entry total max | Unique transaction keys max |',
    '| --- | ---: | ---: | ---: | ---: | ---: | ---: |']
for profile in ['captured-network-diagnostic', 'internal-policy']:
    s = summary['profiles'][profile]
    lines.append(f'| `{profile}` | {s["sdk_rows"]} | {s["transaction_rows"]} | {s["max_sdk"]["instructions"]} | {s["max_sdk"]["memory_bytes"]} | {s["max_sdk_entries"]} | {s["max_unique_footprint"]} |')
lines += ['', 'SDK entry totals differ from unique serialized transaction footprint keys. Maximum values can arise in different records. Internal failures that exported no complete transaction resources remain explicitly unmeasured; no fabricated zero replaces them. Controlled state, synthetic external authorization and unsigned host29 simulation do not establish real signature costs, complete network transaction application or live-state validation.', '',
    '## Verification and reproduction', '',
    'Preservation independently checked GitHub ZIP size/SHA, durable archive size/SHA, exact member-set equality, every member hash/length and byte equality to the original ZIP. It bound profile gates and all error strings to their saved checkpoint logs; checked proof-map and test-source hashes against the exact source commit; validated resource/call/test-log hashes; and parsed completed libtest outcomes. The capture registry identity tests passed4/4 after adding this capture. **A full checker/XDR replay was not performed in this preservation task.** `replay_performed` remains false.', '',
    '[github-metadata.json](github-metadata.json), [capture-identity.json](capture-identity.json), and [checker-job.log](checker-job.log) retain the read-only metadata and checker evidence. [supporting-checksums.json](supporting-checksums.json) binds those files, the verification summary and the local verification helpers. `verify.py` and `finalize.py` preserve the preparation procedure and original target paths; they are not portable replay wrappers.', '',
    'The existing portable offline checker replay, from `peridot-contracts` with locked Node dependencies installed, is:', '',
    '```sh',
    'PYTHONDONTWRITEBYTECODE=1 python3 scripts/replay_cl_linux_ci.py \\',
    '  ../docs/deployment-handoffs/evidence/cl-linux-ci-1198078-20261103 \\',
    '  --output target/cl-linux-ci-1198078-replay',
    '```', '',
    'Use a fresh output directory. Exit1 means the recorded checkpoint failure was reproduced only when both profiles verify; exit2 means verification mismatch. Inspect `replay/replay-verification.json`: the replay compares each gate, exact error multiset and invocation resource/error rows. This command is supplied for reproduction, not reported as already run.', '',
    'No old archive was overwritten. No signatures, broadcasting, secrets, deployment, Upshift binding, borrowing enablement, production maintenance margin, resource-policy change, transaction-stage change or shared00 edit occurred.', '']
(dest / 'README.md').write_text('\n'.join(lines))
assert original == {name: sha(dest / name) for name in original}
print(json.dumps({'archive_sha256': index['archive_sha256'], 'archive_bytes': index['archive_bytes'], 'index_sha256': original['index.json'], 'members': len(index['payload_members'])}, indent=2))
