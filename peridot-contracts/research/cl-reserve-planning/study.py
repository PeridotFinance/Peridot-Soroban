#!/usr/bin/env python3
"""Conditional loss-reserve planning; no venue execution or deployment approval."""
from decimal import Decimal as D, getcontext, ROUND_CEILING
import argparse
import hashlib
import json
from pathlib import Path

getcontext().prec = 60
ZERO, ONE = D(0), D(1)
GROSS = (D(1000), D(5000), D(10000))
SHARES = (D('.50'), D('.30'), D('.20'))
CONTINGENCIES = (ZERO, D('.25'), D('.50'))
LOWER, UPPER = D('.5'), D(2)


def amount(value):
    """Stable human-readable decimal; no binary-float arithmetic."""
    return format(value.quantize(D('0.000000000000000001')), 'f')


def seed_cents(value):
    return format(value.quantize(D('.01'), rounding=ROUND_CEILING), '.2f')


def inventory(gross, price_ratio, strategy):
    """PYUSD amount, USDC amount; initial gross USDC zapped 50/50 at par.

    Fee-free continuous full-range / fixed-band CL. No tick rounding,
    recenter, external-depth, execution-cost or Aquarius equivalence claim.
    """
    assert gross >= ZERO and price_ratio > ZERO
    if strategy == 'full_range':
        return gross / (2 * price_ratio.sqrt()), gross * price_ratio.sqrt() / 2
    assert strategy == 'hypothetical_wide_0.5_2'
    a, b = LOWER.sqrt(), UPPER.sqrt()
    liquidity = gross / (2 * (ONE - a))
    p = min(max(price_ratio.sqrt(), a), b)
    return liquidity * (ONE / p - ONE / b), liquidity * (p - a)


def prices(py, usdc='1'):
    return D(py), D(usdc)


PATHS = {
    'pyusd_down10_persistent': [prices('1'), prices('.9'), prices('.9'), prices('.9'), prices('.9')],
    'pyusd_down25_persistent': [prices('1'), prices('.75'), prices('.75'), prices('.75'), prices('.75')],
    'pyusd_down50_persistent': [prices('1'), prices('.5'), prices('.5'), prices('.5'), prices('.5')],
    'pyusd_down75_persistent': [prices('1'), prices('.25'), prices('.25'), prices('.25'), prices('.25')],
    'pyusd_down50_then_recover': [prices('1'), prices('.5'), prices('.5'), prices('.75'), prices('1')],
    'pyusd_sequential_worsening': [prices('1'), prices('.9'), prices('.75'), prices('.5'), prices('.25')],
    'usdc_down10_persistent': [prices('1'), prices('1', '.9'), prices('1', '.9'), prices('1', '.9'), prices('1', '.9')],
    'both_down25_persistent': [prices('1'), prices('.75', '.75'), prices('.75', '.75'), prices('.75', '.75'), prices('.75', '.75')],
    'both_depeg_pyusd75_usdc50': [prices('1'), prices('.25', '.5'), prices('.25', '.5'), prices('.25', '.5'), prices('.25', '.5')],
}


def path_case(gross, strategy, name, outage_hours):
    """Peak paid shortfall + currently outstanding coverage, in USDC units.

    Initial deposits remain the benchmark: gross USDC, NOT a 50/50 basket.
    Marks are hypothetical fresh values during an outage, not executable oracle
    permissions. Exits at reopening/+1/+2 hours are conditional in-kind payouts.
    """
    path = PATHS[name]
    paid = ZERO
    paid_principal_deficit = ZERO
    paid_owner_shortfall = ZERO
    active = ONE
    peak = ZERO
    peak_hour = 0
    timeline = []
    # The outage starts with the hour1 shock, so24h/72h end at hour25/73.
    for index, ((py, usd), hour) in enumerate(zip(path, (0, 1, outage_hours+1, outage_hours+2, outage_hours+3))):
        x, y = inventory(gross, py / usd, strategy)
        lp_usdc = x * py / usd + y
        hold_usdc = gross
        loss_usdc = max(ZERO, hold_usdc - lp_usdc)
        basket_usdc = gross / 2 * (py / usd + ONE)
        il_diagnostic = max(ZERO, basket_usdc - lp_usdc)
        before = paid + active * loss_usdc
        if before > peak:
            peak, peak_hour = before, hour
        exit_share = SHARES[index-2] if index >= 2 else ZERO
        exit_py, exit_usdc = x * exit_share, y * exit_share
        # Counterfactual full-initial-G diagnostics are not outstanding losses
        # after partial exits. Partition each actual exit and the remaining
        # positions separately; never add this partition to aggregate LP loss.
        debt = gross / 2
        principal_deficit = min(loss_usdc, max(ZERO, debt - lp_usdc))
        owner_shortfall = loss_usdc - principal_deficit
        exit_deficit = exit_share * principal_deficit
        exit_owner_shortfall = exit_share * owner_shortfall
        paid_principal_deficit += exit_deficit
        paid_owner_shortfall += exit_owner_shortfall
        paid += exit_share * loss_usdc
        active -= exit_share
        after = paid + active * loss_usdc
        assert abs(before-after) < D('1e-50')
        remaining_deficit = active * principal_deficit
        remaining_owner_shortfall = active * owner_shortfall
        assert abs(paid_principal_deficit + paid_owner_shortfall - paid) < D('1e-50')
        assert abs(paid_principal_deficit + paid_owner_shortfall + remaining_deficit
                   + remaining_owner_shortfall - after) < D('1e-50')
        timeline.append({
            'hour': hour, 'pyusd_usd': amount(py), 'usdc_usd': amount(usd),
            'fresh_execution_eligible': False,
            'exit_assumed_available_after_outage': index >= 2,
            'oracle_or_venue_outage': 0 < index < 2,
            'original_deposit_hold_value_usdc': amount(hold_usdc),
            'modeled_lp_value_usdc_before_position_exits': amount(lp_usdc),
            'aggregate_lp_shortfall_usdc_before_position_exits': amount(loss_usdc),
            'basket_il_diagnostic_usdc_NOT_coverage': amount(il_diagnostic),
            'exit_fraction_initial_gross': amount(exit_share),
            'in_kind_exit_pyusd_tokens': amount(exit_py),
            'in_kind_exit_usdc_tokens': amount(exit_usdc),
            'in_kind_mark_usdc_NO_conversion_promise': amount(exit_py * py / usd + exit_usdc),
            'cumulative_paid_reserve_claims_usdc': amount(paid),
            'remaining_position_fraction': amount(active),
            'unexited_shortfall_usdc': amount(active * loss_usdc),
            'cumulative_required_reserve_usdc': amount(after),
            'two_x_full_initial_gross_counterfactual_principal_collateral_deficit_usdc': amount(principal_deficit),
            'two_x_full_initial_gross_counterfactual_owner_shortfall_usdc': amount(owner_shortfall),
            'two_x_exit_principal_collateral_deficit_usdc': amount(exit_deficit),
            'two_x_exit_owner_shortfall_usdc': amount(exit_owner_shortfall),
            'two_x_cumulative_principal_collateral_deficit_usdc': amount(paid_principal_deficit),
            'two_x_cumulative_owner_shortfall_usdc': amount(paid_owner_shortfall),
            'two_x_unexited_principal_collateral_deficit_usdc': amount(remaining_deficit),
            'two_x_unexited_owner_shortfall_usdc': amount(remaining_owner_shortfall),
            'two_x_realized_lender_bad_debt': 'UNPROVEN; zero only conditional on executed full funded repayment; collateral deficit is borne by payer/liquidator unless covered',
            'two_x_exiting_principal_only_funded_liquidator_cash_usdc': amount(exit_share * debt),
        })
    assert active == ZERO
    return {'gross_cl_usdc_initial_usd_par': amount(gross), 'strategy': strategy,
            'path': name, 'outage_hours': outage_hours, 'outage_start_hour': 1,
            'conditional_reopening_hour': outage_hours+1,
            'third_party_liquidity_removal_fraction': '0.50',
            'liquidity_removal_cost_or_capacity_modeled': False,
            'peak_loss_only_reserve_usdc': amount(peak), 'peak_hour': peak_hour,
            'final_paid_shortfall_usdc': amount(paid), 'timeline': timeline,
            'coverage_status': 'UNPROVEN', 'conditional_exits_not_execution_evidence': True}


def study(total_cost_usdc=None):
    if total_cost_usdc is not None and total_cost_usdc < ZERO:
        raise ValueError('Cost sensitivity must be nonnegative')
    cases, summary = [], []
    for gross in GROSS:
        for strategy in ('full_range', 'hypothetical_wide_0.5_2'):
            subset = [path_case(gross, strategy, name, hours) for name in PATHS for hours in (24, 72)]
            cases.extend(subset)
            worst = max(subset, key=lambda c: D(c['peak_loss_only_reserve_usdc']))
            loss = D(worst['peak_loss_only_reserve_usdc'])
            summary.append({
                'gross_cl_usdc_initial_usd_par': amount(gross), 'strategy': strategy,
                'worst_named_path': worst['path'], 'loss_only_floor_usdc_exact': amount(loss),
                'loss_only_seed_usdc_rounded_up': {str(c): seed_cents(loss*(ONE+c)) for c in CONTINGENCIES},
                'all_in_formula': '(peak cumulative LP shortfall + C_total_necessary_costs_once) * (1 + contingency)',
                'unknown_cost_default_is_NOT_zero': total_cost_usdc is None,
                'user_supplied_cost_sensitivity_usdc_NOT_measured': None if total_cost_usdc is None else amount(total_cost_usdc),
                'conditional_all_in_sensitivity_usdc': None if total_cost_usdc is None else {str(c): seed_cents((loss+total_cost_usdc)*(ONE+c)) for c in CONTINGENCIES},
                'guaranteed_all_in_seed_usdc': None, 'safe_cap': None, 'coverage_status': 'UNPROVEN',
            })
    capital = []
    for gross in GROSS:
        for leverage in (D(1), D(2)):
            for allocation in (D('.75'), D('.90')):
                equity = gross / leverage
                supplier = equity / allocation
                debt = gross - equity
                capital.append({
                    'gross_cl_usdc': amount(gross), 'leverage_scenario': int(leverage),
                    'strategy_allocation_fraction_of_supplier_capital': str(allocation),
                    'strategy_owner_equity_usdc': amount(equity), 'borrowed_principal_usdc': amount(debt),
                    'supplier_total_capital_usdc': amount(supplier),
                    'supplier_withdrawal_cash_if_no_other_uses_usdc': amount(supplier-equity),
                    'debt_funding_source_assumption': 'Distinct lender cash outside strategy supplier capital S; never also supplier withdrawal or loss reserve cash',
                    'same_supplier_pool_cash_after_equity_and_borrow_usdc': amount(max(ZERO, supplier-equity-debt)),
                    'same_supplier_pool_additional_funding_gap_usdc': amount(max(ZERO, equity+debt-supplier)),
                    'gross_counted_once_check': amount(equity+debt),
                    'concurrent_position_gross_usdc': [amount(gross*f) for f in SHARES],
                    'largest_first_liquidator_principal_tranches_usdc': [amount(debt*f) for f in SHARES],
                    'liquidator_cash_if_no_recycling_principal_only_usdc': amount(debt),
                    'accrued_interest_and_liquidator_transaction_costs': 'UNKNOWN: additional funding; interest is internal aggregate transfer, not new external LP loss',
                    'two_x_admission_approved': False,
                    'operating_budget': 'SEPARATE_UNQUANTIFIED', 'loss_seed': 'SEPARATE_FROM_SUPPLIER_AND_LIQUIDATOR_CASH',
                })
    return {'schema': 'stellar-cl-reserve-planning-v1', 'coverage_status': 'UNPROVEN',
            'release_approval': False, 'stage1_complete': False, 'production_parameters_changed': False,
            'source_sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
            'assumptions': {
                'gross_exposure': '$1000/$5000/$10000 per pool at initial USDCUSD=1, includes borrowed principal once; planning values, not actual caps',
                'coverage': '100% modeled aggregate LP shortfall versus HOLD ORIGINAL USDC DEPOSITS plus necessary costs once',
                'initial_zap': 'Costless conversion to50/50 PYUSD/USDC at initial prices1USD; real zap cost UNKNOWN in C',
                'wide_band': '[0.5,2] PYUSD/USDC relative price, fixed/no recenter, hypothetical not Aquarius tick config',
                'fees_rewards': 'No fee/reward credit in loss-only model, no APR forecast; actual income/missed fees unknown',
                'future_income_day1_credit_usdc': '0', 'existing_dedicated_seed_assets_verified': False,
                'existing_seed_credit_usdc': '0 (no verified credit, not assertion that actual balances are zero)',
                'future_replenishment': '50% actually collected eligible future income only under prospective approved ownership policy',
                'liquidity_stress': '50% third-party removal at hour1; actual depth, fees, price impact, fillability UNKNOWN',
                'outage': '24h and72h; no executable exits during outage; assumed in-kind exits at reopening/+1/+2h are conditional',
                'exit_order': 'concurrent50/30/20% gross positions, largestfirst; no fee/reward netting against booked claims',
                'seed_units': 'USDC; lossUSD/usdcUSD each boundary; no guarantee USDC preservesUSD purchasingpower',
                'costs': 'C includes necessary zap/unwind/recovery/slippage/resource/restoration costs ONCE; omitted argument means UNKNOWN not zero',
                'contingency': '0/25/50% on modeled loss+C; scenario margins not probability estimates',
            }, 'conditional_seed_summaries': summary, 'capital_purposes_grid': capital, 'cases': cases}


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--output', type=Path, default=Path(__file__).with_name('results.json'))
    p.add_argument('--cost-usdc', type=D, help='Explicit illustrative total necessary per-pool cost across all exits; NOT a measured fee quote')
    args = p.parse_args()
    result = study(args.cost_usdc)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2)+'\n')
    for row in result['conditional_seed_summaries']:
        print(row['gross_cl_usdc_initial_usd_par'], row['strategy'], row['loss_only_seed_usdc_rounded_up'], 'coverage UNPROVEN; add unknownC once')


if __name__ == '__main__':
    main()
