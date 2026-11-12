#!/usr/bin/env python3
"""Summarize recorded host call trees without modifying execution or limits.

Inclusive rows overlap. Host25 memory measures cumulative modeled charges;
host29 attributes growth of an invocation-wide high-water mark with refunds,
not self allocations or independent call peaks. Neither measures native RSS.
Ledger net deltas are not transaction footprint estimates.
"""
import argparse
from collections import Counter, defaultdict
import json
from pathlib import Path


def summarize(row):
    required = {'schema', 'flow', 'profile', 'run_mode', 'calls',
                'root_instructions', 'root_memory_bytes', 'exclusive_reconciles',
                'runtime_protocol', 'memory_semantics', 'diagnostic_descriptors_complete',
                'shadow_instrumentation'}
    if required - row.keys() or row['schema'] != 'cl-call-profile-v2':
        raise ValueError('missing or unsupported profiling schema')
    if not row['calls'] or not row['exclusive_reconciles']:
        raise ValueError('empty or unreconciled call tree')
    if not row['diagnostic_descriptors_complete']:
        raise ValueError('incomplete call descriptors: verify diagnostic level and shadow instrumentation budget')
    shadow = row['shadow_instrumentation']
    if shadow['cpu_limit_exceeded'] or shadow['memory_limit_exceeded']:
        raise ValueError('shadow instrumentation exhausted: per-call attribution evidence is incomplete')
    expected_semantics = {25: 'cumulative_modeled_memory_charges',
                          29: 'global_high_water_mark_growth_with_refunds'}
    if expected_semantics.get(row['runtime_protocol']) != row['memory_semantics']:
        raise ValueError('unreviewed or inconsistent runtime memory semantics')
    memory_field = 'memory_counter_contribution_bytes'
    roles = defaultdict(lambda: dict(calls=0, instructions=0, memory_counter_contribution_bytes=0))
    functions = Counter()
    for call in row['calls']:
        costs = call['exclusive']
        role = roles[call['role']]
        role['calls'] += 1
        for name in ('instructions', memory_field):
            if type(costs[name]) is not int:
                raise ValueError(f'noninteger {name}')
            role[name] += costs[name]
        functions[(call['role'], call['function'])] += 1
    for resource, root in [('instructions', 'root_instructions'), (memory_field, 'root_memory_bytes')]:
        if sum(role[resource] for role in roles.values()) != row[root]:
            raise ValueError(f'{resource} breakdown does not reconcile')
    return {
        'flow': row['flow'], 'profile': row['profile'], 'run_mode': row['run_mode'],
        'invocation_ok': row['invocation_ok'],
        'runtime_protocol': row['runtime_protocol'], 'memory_semantics': row['memory_semantics'],
        'shadow_instrumentation': shadow,
        'root_instructions': row['root_instructions'], 'root_memory_bytes': row['root_memory_bytes'],
        'attribution_by_role': dict(sorted(roles.items(), key=lambda x: -x[1][memory_field])),
        'repeated_calls': [{'role': role, 'function': function, 'count': count}
                           for (role, function), count in functions.most_common() if count > 1],
        'largest_inclusive_calls': sorted(row['calls'], key=lambda c: -c['inclusive']['memory_bytes'])[:20],
        'largest_memory_cost_types': sorted(row.get('cost_type_trackers', []), key=lambda c: -c['memory_bytes'])[:12],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('report', type=Path)
    parser.add_argument('--flow')
    parser.add_argument('--json', action='store_true')
    args = parser.parse_args()
    rows = [json.loads(line) for line in args.report.read_text().splitlines() if line.strip()]
    if args.flow:
        rows = [row for row in rows if row['flow'] == args.flow]
    if not rows:
        parser.error('no measured flows match; missing profiling evidence is not zero cost')
    results = [summarize(row) for row in rows]
    if args.json:
        print(json.dumps(results, indent=2))
        return
    for result in results:
        print(f"{result['flow']} / {result['profile']} / {result['run_mode']} "
              f"ok={result['invocation_ok']} CPU={result['root_instructions']} "
              f"modeled_memory={result['root_memory_bytes']} runtime={result['runtime_protocol']} "
              f"semantics={result['memory_semantics']}")
        print('Role | Calls | Exclusive CPU | Memory counter contribution')
        for role, costs in result['attribution_by_role'].items():
            print(f"{role} | {costs['calls']} | {costs['instructions']} | {costs['memory_counter_contribution_bytes']}")
        print('Repeated calls: ' + ', '.join(f"{r['role']}.{r['function']}={r['count']}"
                                           for r in result['repeated_calls']))
        print('Budget-interval cumulative cost type charges (not invocation peak): ' + ', '.join(f"{r['cost_type']}={r['memory_bytes']}"
                                                for r in result['largest_memory_cost_types']))
        print('Inclusive call totals overlap; ledger net deltas are not footprint sums.')
        if result['runtime_protocol'] == 29:
            print('Memory attribution is global high-water growth, not independent call peaks or self allocations.')


if __name__ == '__main__':
    main()
