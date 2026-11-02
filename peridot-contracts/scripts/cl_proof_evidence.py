"""Validate completed lifecycle tests separately from invocation resource rows.

A successful invocation is not a completed proof: later assertions can fail.
Manifests bind explicit proof labels to source, raw Cargo logs and the resource
report. They neither grant release approval nor turn missing tests into passes.
"""
import hashlib
import json
import re
from pathlib import Path


def sha256(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def verified_file(root, entry):
    path = (root / entry['path']).resolve()
    if sha256(path) != entry['sha256']:
        raise ValueError('evidence file hash mismatch: ' + str(path))
    return path


def cargo_outcomes(text):
    """Parse single-threaded libtest output, allowing captured output per test."""
    starts = list(re.finditer(r'^test ([A-Za-z0-9_:]+) \.\.\. ', text, re.M))
    result = {}
    for i, match in enumerate(starts):
        end = starts[i + 1].start() if i + 1 < len(starts) else len(text)
        body = text[match.end():end]
        # The status is the last standalone libtest outcome before the next test.
        outcomes = re.findall(r'^(ok|FAILED|ignored)(?:\s.*)?$', body, re.M)
        result[match[1]] = ('DUPLICATE' if match[1] in result else
                            outcomes[-1] if outcomes else 'UNFINISHED')
    return result


def validate_manifest(path, report_path, profile, valid_flows, required):
    errors = []
    if path is None:
        return ['completed lifecycle proof manifest missing']
    try:
        path = Path(path)
        manifest = json.loads(path.read_text())
        root = path.parent
        expected = {
            'schema': 'cl-lifecycle-proof-v1',
            'profile': profile['name'], 'config_sha256': profile['network_sha256'],
            'resource_report_sha256': sha256(report_path),
        }
        for key, value in expected.items():
            if manifest.get(key) != value:
                errors.append('proof manifest mismatch: ' + key)
        mapping = json.loads(verified_file(root, manifest['reviewed_map']).read_text())
        if mapping['schema'] != 'cl-proof-map-v1' or mapping['proofs'] != manifest['proofs'] or set(mapping['sources']) != set(manifest['sources']):
            errors.append('reviewed proof map differs from manifest')
        for test, source in mapping['sources'].items():
            expected_path = (Path(manifest['source_root']) / source).resolve()
            actual_path = (root / manifest['sources'][test]['path']).resolve()
            if expected_path != actual_path:
                errors.append('reviewed source path mismatch: ' + test)
        if 'call_profile' in manifest:
            contracts = {}
            for line in verified_file(root, manifest['call_profile']).read_text().splitlines():
                for call in json.loads(line)['calls']:
                    role, address = call.get('role'), call.get('address')
                    if role and role != 'unlabelled' and address:
                        contracts.setdefault(role, set()).add(address)
            if manifest.get('contracts') != {r: sorted(v) for r, v in contracts.items()}:
                errors.append('fixture contract role map mismatch')
        outcomes = {}
        runs = manifest['test_runs']
        if not isinstance(runs, list) or not runs:
            raise ValueError('test run evidence missing')
        for run in runs:
            log = verified_file(root, run).read_text()
            if type(run.get('exit_code')) is not int or run['exit_code'] != 0:
                errors.append('test process did not succeed: ' + run['path'])
            if not re.search(r'^test result: ok\. \d+ passed; 0 failed;', log, re.M):
                errors.append('successful Cargo suite summary missing: ' + run['path'])
            parsed = cargo_outcomes(log)
            for test, status in parsed.items():
                outcomes.setdefault(test, []).append(status)
        sources = manifest['sources']
        for test, entry in sources.items():
            source = verified_file(root, entry).read_text()
            function = test.rsplit('::', 1)[-1]
            if not function.startswith('compiled_') or not re.search(r'\bfn\s+' + re.escape(function) + r'\s*\(', source):
                errors.append('compiled test source mismatch: ' + test)
        proofs = manifest['proofs']
        if not isinstance(proofs, list):
            raise ValueError('proof mappings missing')
        labels = set()
        for proof in proofs:
            label = proof['label']
            if not isinstance(label, str) or label in labels:
                raise ValueError('invalid or duplicate proof label')
            labels.add(label)
            resource_flows = proof.get('resource_flows', [label])
            if not isinstance(resource_flows, list) or not resource_flows or not all(isinstance(v, str) for v in resource_flows):
                raise ValueError('invalid invocation mapping: ' + label)
            for flow in resource_flows:
                if flow not in valid_flows:
                    errors.append('proof has no successful measured invocation: ' + label + ' -> ' + flow)
            if not isinstance(proof.get('assertions'), list) or not proof['assertions'] or not all(isinstance(v, str) and v.strip() for v in proof['assertions']):
                errors.append('proof assertion description missing: ' + label)
            tests = proof['tests']
            if not isinstance(tests, list) or not tests:
                raise ValueError('proof tests missing: ' + label)
            for test in tests:
                if test not in sources or not outcomes.get(test) or any(v != 'ok' for v in outcomes[test]):
                    errors.append('proof test not completed successfully: ' + label + ' -> ' + test)
        missing = set(required) - labels
        if missing:
            errors.append('proof mappings missing: ' + ', '.join(sorted(missing)))
    except (OSError, KeyError, TypeError, ValueError) as error:
        errors.append('invalid lifecycle evidence: ' + str(error))
    return errors
