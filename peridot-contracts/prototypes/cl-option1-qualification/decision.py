"""Analysis only: no new resource profile or ceiling is installed."""
import hashlib,json,lzma
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
file=ROOT.parent/'docs/deployment-handoffs/evidence/cl-restoration-preparation-20261103/assessment.json'
a=json.loads(file.read_text());rows=[r for r in a['results'] if r['profile']=='captured-network-diagnostic']
peak={
 'CPU':max(max(r['host_max_completed']['instructions'],r['sdk_max_attempted']['instructions']) for r in rows),
 'Memory bytes':max(max(r['host_max_completed']['memory_bytes'],r['sdk_max_attempted']['memory_bytes']) for r in rows),
 'SDK entries':max(r['sdk_entries_max_attempted'] for r in rows),
 'Unique transaction keys':max(r['unique_keys_max_completed'] for r in rows),
 'Writes':max(max(r['host_max_completed']['write_entries'],r['sdk_max_attempted']['write_entries']) for r in rows)}
settings={'CPU':(100000000,300000000,240000000,400000000),'Memory bytes':(41943040,33554432,26843545,41943040),'SDK entries':(100,300,240,None),'Unique transaction keys':(100,300,240,400),'Writes':(50,100,80,200)}
text='# Option1 qualification decision table (unadopted)\n\nMeasured maxima are independent, controlled-state unsigned prepared roots from9bb51aa. They are not signed Core application or live capacity.\n\n| Dimension | Unchanged internal | Option1 ceiling | Option1 workload | Measured maximum | Remaining workload headroom | Captured network |\n|---|---:|---:|---:|---:|---:|---:|\n'
for k,(internal,ceiling,boundary,network) in settings.items():text+=f'| {k} | {internal:,} | {ceiling:,} | {boundary:,} | {peak[k]:,} | {boundary-peak[k]:,} | {network if network is not None else "No SDK-total field"} |\n'
text+='''
| Alternative | Concrete dependency | Missing costs/proofs | Recommendation |
|---|---|---|---|
| Retain100M/100/50 | Required Aquarius fee/position operations must change: current venue-exclusive open162.817M and close137.760M exceed100M before Peridot work. A cheaper fee-inclusive valuation/liquidity ABI must preserve actual claims and exact fee ownership. Receipt/controller accesses must also fit100 SDK entries and100 actual keys. | Changed venue correctness, fee/reward entitlement and complete exit/recovery costs; no saving credited. | Stop wrapper tuning. Keep this redesign narrowly scoped; no new financial-stage split. |
| Option1, unadopted | Preserve atomic repayment/seizure and claim-first boundaries; qualify explicit restoration and every pending-state recovery with bounded new-risk admission. Never reject existing recovery or prune entitlement history to enforce new-risk bounds. | Core application, real signatures/accounts and signed envelope costs, live archive discovery, elapsed preparation/re-expiry, sequential shared recovery, more expensive fresh-price branches and capacity. Largest CPU margin is only12.088M/5.04% of240M. | Technically plausible for measured roots; not qualified. Economic worth depends on amortized rent, funded recovery and actual collected income, not on consuming the20% reserve. |

Native RestoreFootprint has separate applicable footprint/byte/fee checks. A declared0 instruction field is not native CPU/memory measurement. All other unchanged checks (disk reads/bytes, write bytes, events/return, entry sizes, fees, missing flows) remain mandatory. No boundary is increased by this analysis.
'''
prior=ROOT.parent/'docs/deployment-handoffs/evidence/cl-resource-decision-refresh-20261102/analysis.json.xz'
old=json.loads(lzma.open(prior).read())
text+='\n## Preserved 1× supplier evidence, a different measured population\n\nDeposit and redeem are required even with borrowing disabled. These are preserved atomic baseline measurements across the separately identified Mac/Linux artifacts, not reruns of the enlarged pending-recovery V3. Their existing resource failures remain.\n\n| Root | CPU maximum | Memory maximum | SDK entries | Unique keys | Writes | CPU headroom to proposed240M |\n|---|---:|---:|---:|---:|---:|---:|\n'
supplier={}
for name in ['deposit_underlying','redeem']:
 flows=[b['flows'][name] for b in old['atomic_builds'].values()]
 cpu=max(max(f['sdk_maxima']['instructions'],f['unsigned_transaction_maxima']['instructions']) for f in flows)
 memory=max(max(f['sdk_maxima']['memory_bytes'],f['unsigned_transaction_maxima']['memory_bytes']) for f in flows)
 sdk=max(f['sdk_maxima']['sdk_entry_total'] for f in flows)
 keys=max(f['unsigned_transaction_maxima']['unique_footprint_keys'] for f in flows)
 writes=max(max(f['sdk_maxima']['write_entries'],f['unsigned_transaction_maxima']['write_entries']) for f in flows)
 supplier[name]={'instructions':cpu,'memory_bytes':memory,'sdk_entries':sdk,'unique_keys':keys,'writes':writes}
 text+=f'| {name} | {cpu:,} | {memory:,} | {sdk} | {keys} | {writes} | {240000000-cpu:,} |\n'
text+='\nRequired venue/accounting changes under100M must therefore cover supplier mint and redemption as well as margin open/close. A wider range does not remove fee-inclusive NAV, claims, ownership accounting or exit costs. Signatures, fresh live state and a joined worst-case workload remain unmeasured.\n'
HERE=Path(__file__).resolve().parent
(HERE/'decision-table.md').write_text(text)
(HERE/'decision-table.json').write_text(json.dumps({'source':str(file),'source_sha256':hashlib.sha256(file.read_bytes()).hexdigest(),'measured_maxima':peak,'settings':settings,'preserved_supplier_source_sha256':hashlib.sha256(prior.read_bytes()).hexdigest(),'preserved_supplier_maxima':supplier,'policy_adopted':False,'signed':False,'release_approval':False},indent=2)+'\n')
