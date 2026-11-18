# Evidence kept out of git

Run outputs are not committed (repository hygiene rule). Copies live in the
worktree's ignored `target/clg/evidence/`; the originals remain in the main
checkout at the paths below.

| File | SHA-256 | Original location |
|---|---|---|
| group-results-20261118.json (277,731 B) | 29013edb9f6e9fd38a9e6212e6c599c9faf1cae85402ba1fc1bb7f09bdf71555 | peridot-contracts/research/cl-collateral-group-20261118/results.json |
| spike-results-20261117.json (84,918 B) | e63ca35c76eb57cc9dad12cb2f4f96d33bdd0385156a6cfb33596db17a81e4cf | peridot-contracts/research/cl-collateral-spike-20261117/results.json |

`rows-table.md` in each folder is the human-readable summary of those rows.
