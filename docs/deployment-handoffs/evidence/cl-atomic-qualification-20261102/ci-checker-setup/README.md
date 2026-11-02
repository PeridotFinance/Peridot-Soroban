# Required CI decoder dependencies

The checker job atc80a1da failed because its new real-XDR regressions could not
load the locked Stellar JavaScript SDK. This is an ordinary CI setup failure,
not an expected resource-policy failure. The original job log is preserved here.

The follow-up adds the same hash-pinned Node22 setup and `npm ci --ignore-scripts`
used by the evidence job. No tests, gates or limits are skipped. A fresh local
lockfile-only offline installation passes all10 new regressions; the full local
suite previously passed42. Next Linux execution remains a separate result.

[index.json](index.json) binds the exact failing run/job, preserved logs and local
rehearsal. No signing, broadcasting, secrets, production configuration or stage
changes. The atomic measurement and previous CI archives remain unchanged.
