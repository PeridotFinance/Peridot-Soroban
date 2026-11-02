"""Explicit identities of reviewed CI captures; no inferred/latest artifact selection."""
CAPTURES = {
    '90111e2': {
        'commit': '90111e2dc1a3cc9ffd941eb3661459e6441e1280',
        'merge_commit': '57d27fb5abd6244e67a8f74a1dc0f1e5b79fc044',
        'run_url': 'https://github.com/PeridotFinance/Peridot-Soroban/actions/runs/41492773441',
        'artifact_id': 13167198650,
        'original_expiry': '2026-11-16T18:21:25Z',
        'original_zip_sha256': 'a041000a38963ceed67bcb7b13f501622f97ba77fbb36817ca3ebf6adee760f9',
    },
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
    '9cf4308': {
        'commit': '9cf43082bfb57334d57dbc58cfa90ccad4d98078',
        'merge_commit': '8b7d356e6a5edf9c53540f302cd142084a426c9a',
        'run_url': 'https://github.com/PeridotFinance/Peridot-Soroban/actions/runs/41473099204',
        'artifact_id': 13158618482,
        'original_expiry': '2026-11-16T16:18:07Z',
        'original_zip_sha256': 'a9fdb21a4c0327609a09f162859ce31259d64b54905c06948588fe8e711b2d83',
    },
    'bb154ec': {
        'commit': 'bb154ec8f03889aabfdb8dd5ea120f22c46a47f5',
        'merge_commit': '966d39f6771473a3e724524beed5d65f8e1deee7',
        'run_url': 'https://github.com/PeridotFinance/Peridot-Soroban/actions/runs/41484260698',
        'artifact_id': 13163838118,
        'original_expiry': '2026-11-16T17:25:37Z',
        'original_zip_sha256': '5299122088d4efa52194de035aac76b6bedc6f9801a03c5c73694af71e200a43',
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
