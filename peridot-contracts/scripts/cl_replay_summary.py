"""Exact gate/error replay comparison. Expected failure is never release approval."""
from collections import Counter

FIELDS = {'schema', 'profile', 'config_sha256', 'gates', 'selected_gate',
          'release_approval', 'missing_required_flows', 'lifecycle_errors', 'transaction_errors'}
ERRORS = ('missing_required_flows', 'lifecycle_errors', 'transaction_errors')

def compare(expected, observed, exit_code, paths=()):
    for value in (expected, observed):
        if set(value) != FIELDS or value['schema'] != 'cl-verification-gates-v1':
            raise ValueError('missing or unexpected summary fields')
        gates = value['gates']
        if set(gates) != {'invocation', 'lifecycle', 'transaction', 'checkpoint'} or any(type(v) is not bool for v in gates.values()):
            raise ValueError('invalid gate summary')
        if gates['checkpoint'] != all(gates[k] for k in ('invocation', 'lifecycle', 'transaction')):
            raise ValueError('inconsistent checkpoint summary')
        if value['release_approval'] is not False or value['selected_gate'] != 'checkpoint':
            raise ValueError('unexpected approval or selected gate')
        for key in ERRORS:
            if not isinstance(value[key], list) or any(not isinstance(s, str) for s in value[key]):
                raise ValueError('invalid error list: ' + key)
    if exit_code != (0 if observed['gates']['checkpoint'] else 1):
        raise ValueError('checker process status disagrees with gate summary')
    def normalized(s):
        for old, new in sorted(paths, key=lambda pair: len(pair[0]), reverse=True):
            s = s.replace(old, new)
        return s
    for key in FIELDS:
        if key in ERRORS:
            same = Counter(map(normalized, expected[key])) == Counter(map(normalized, observed[key]))
        else:
            same = expected[key] == observed[key]
        if not same:
            raise ValueError('replay summary differs: ' + key)
    return {'verified': True, 'gates': observed['gates'], 'errors': {k: observed[k] for k in ERRORS}, 'release_approval': False}


def invocation_block(text):
    """Bind each invocation error AND its resource/fee row in the saved raw log."""
    lines = text.splitlines()
    if not lines or not lines[0].startswith('PROFILE '):
        raise ValueError('missing invocation log profile')
    for index, line in enumerate(lines):
        if line.startswith(('UNMEASURED:', 'INTERNAL-POLICY:', 'CAPTURED-NETWORK-DIAGNOSTIC:')):
            if not any(': PASS cpu=' in v or ': FAIL cpu=' in v for v in lines[:index]):
                raise ValueError('missing invocation rows')
            return lines[:index]
    raise ValueError('missing invocation log terminator')

def compare_invocations(expected_log, observed_log):
    if invocation_block(expected_log) != invocation_block(observed_log):
        raise ValueError('replay invocation resources/errors differ')
    return True
