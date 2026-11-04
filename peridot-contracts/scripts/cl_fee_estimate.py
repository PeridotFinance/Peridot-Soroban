"""Local measured resource estimates at captured public-network fee rates.

Not an RPC transaction simulation: envelope/history/base fees and return-value
bytes are excluded. Resource quantities come from execution, not WASM size.
"""
import json
from pathlib import Path

def ceil(n, d):
    return (n + d - 1) // d

def estimate(row, config_path):
    cfg = json.loads(Path(config_path).read_text())
    s = {key: value['values'] for key, value in cfg['settings'].items()}
    c, l = s['configSettingContractComputeV0'], s['configSettingContractLedgerCostV0']
    ext, a = s['configSettingContractLedgerCostExtV0'], s['configSettingStateArchival']
    window = s['configSettingLiveSorobanStateSizeWindow']
    sizes = list(window.values()) if isinstance(window, dict) else window
    if not isinstance(sizes, list) or not sizes or any(type(n) is not int or n < 0 for n in sizes):
        raise ValueError('missing or invalid live-state size window')
    state_size = sum(sizes) // len(sizes)
    target = l['sorobanStateTargetSizeBytes']
    multiplier = l['rentFee1KbSorobanStateSizeHigh'] - l['rentFee1KbSorobanStateSizeLow']
    if state_size < target:
        rent_rate = ceil(multiplier * state_size, target) + l['rentFee1KbSorobanStateSizeLow']
    else:
        rent_rate = l['rentFee1KbSorobanStateSizeHigh'] + ceil(multiplier * (state_size-target) * l['sorobanStateRentFeeGrowthFactor'], target)
    rent_rate = max(1000, rent_rate)
    components = {
        'cpu': ceil(row['instructions'] * c['feeRatePerInstructionsIncrement'], 10_000),
        'disk_entry_reads': (row['disk_read_entries'] + row['write_entries']) * l['feeDiskReadLedgerEntry'],
        'entry_writes': row['write_entries'] * l['feeWriteLedgerEntry'],
        'read_bytes': ceil(row['read_bytes'] * l['feeDiskRead1Kb'], 1024),
        'write_bytes': ceil(row['write_bytes'] * ext['feeWrite1Kb'], 1024),
        'events': ceil(row['contract_events_size_bytes'] * s['configSettingContractEventsV0']['feeContractEvents1Kb'], 1024),
    }
    for kind, denom in [('persistent', a['persistentRentRateDenominator']), ('temporary', a['tempRentRateDenominator'])]:
        bumps = row[f'{kind}_entry_rent_bumps']
        components[kind + '_rent'] = ceil(row[f'{kind}_rent_ledger_bytes'] * rent_rate, 1024 * denom) + bumps * l['feeWriteLedgerEntry'] + ceil(48 * bumps * ext['feeWrite1Kb'], 1024)
    return {'resource_fee_stroops': sum(components.values()), 'components': components,
            'captured_ledger': cfg['ledger'], 'network_protocol': cfg['network']['protocolVersion'],
            'local_host_protocol': row['runtime_protocol'], 'rent_rate_per_1kb': rent_rate,
            'runtime_matches_network': row['runtime_protocol'] == cfg['network']['protocolVersion'],
            'profile': row['profile'], 'run_mode': row['run_mode'],
            'full_transaction_fee': False,
            'scope': 'partial local resource estimate; excludes envelope/history/base fees, return bytes; not RPC simulation'}
