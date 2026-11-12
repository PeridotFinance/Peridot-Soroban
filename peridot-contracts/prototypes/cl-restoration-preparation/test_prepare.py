"""Wrong source/build identities must fail before a combined bundle is made."""
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
import unittest

HERE = Path(__file__).resolve().parent

class PrepareIdentityTests(unittest.TestCase):
    def test_incompatible_build_is_never_relabelled_combined(self):
        overlay = (HERE.parent / 'cl-atomic-qualification/optimization.patch').read_bytes()
        overlay += (HERE.parent / 'cl-storage-qualification/storage.patch').read_bytes()
        overlay += (HERE.parent / 'cl-storage-qualification/repayment.patch').read_bytes()
        valid = {'optimization_sha256': hashlib.sha256(overlay).hexdigest(),
                 'margin_build_feature': 'funded-liquidation-validation',
                 'contract_cache_scope': 'fresh build-specific directory; never reuse previous workspace contract outputs'}
        for field, value in [('optimization_sha256', '0' * 64),
                             ('margin_build_feature', 'default'),
                             ('contract_cache_scope', 'shared old contract cache')]:
            with self.subTest(field=field), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                build = root / 'input'
                build.mkdir()
                (build / 'build-manifest.json').write_text(json.dumps({**valid, field: value}))
                result = subprocess.run(['python3', str(HERE / 'prepare.py'), '--build', str(build),
                                         '--output', str(root / 'output')], capture_output=True, text=True)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn('AssertionError', result.stderr)
                self.assertFalse((root / 'output').exists())

if __name__ == '__main__':
    unittest.main()
