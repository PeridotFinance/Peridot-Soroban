"""Explicit identities of reviewed CI captures; no inferred/latest artifact selection."""
CAPTURES = {
    'da4e1a5': {
        'commit': 'da4e1a598b7c652c79745836417d9943e561c6f2',
        'merge_commit': 'e42e3067795124f457b43e5e4d0e2893f7d22815',
        'run_url': 'https://github.com/PeridotFinance/Peridot-Soroban/actions/runs/41434868065',
        'artifact_id': 13136354990,
        'original_expiry': '2026-11-16T11:30:36Z',
        'original_zip_sha256': '9b01db8227a07f0b8a4bc13dfbf9b043002df79f0c6de068d126e3dd74d87d03',
    },
    'ac63255': {
        'commit': 'ac6325589a0095084c2df641d9b743860b4795f8',
        'merge_commit': '904f2aa7a106dbcfe6d3af5c392dec434ab9a143',
        'run_url': 'https://github.com/PeridotFinance/Peridot-Soroban/actions/runs/41449397462',
        'artifact_id': 13146343183,
        'original_expiry': '2026-11-16T13:40:11Z',
        'original_zip_sha256': '33dcb99170894384466ebe6ac9d0e3d2fc95dfeaec599fc2066eb682f6a56701',
    },
}


def validate_identity(index):
    matches = [v for v in CAPTURES.values() if all(index.get(k) == x for k, x in v.items())]
    if len(matches) != 1:
        raise ValueError('historical index identity differs from an explicitly pinned capture')
    if (index.get('schema') != 'cl-linux-ci-archive-v1'
            or index.get('release_approval') is not False
            or set(index.get('profiles', {})) != {'internal-policy', 'captured-network-diagnostic'}):
        raise ValueError('invalid historical index schema/profiles/approval')
    return matches[0]
