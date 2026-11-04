import unittest
from decimal import Decimal as D
import study


class PlanningInvariantTests(unittest.TestCase):
    def test_initial_inventory_and_single_underlying_benchmark(self):
        for model in ('full_range','hypothetical_wide_0.5_2'):
            x,y=study.inventory(D(1000),D(1),model)
            self.assertLess(abs(x-D(500)),D('1e-50'))
            self.assertLess(abs(y-D(500)),D('1e-50'))
        case=study.path_case(D(1000),'full_range','pyusd_down50_persistent',24)
        row=case['timeline'][1]
        self.assertGreater(D(row['aggregate_lp_shortfall_usdc_before_position_exits']),D(292))
        self.assertLess(D(row['basket_il_diagnostic_usdc_NOT_coverage']),D(43))

    def test_wide_boundaries_and_nonnegative_assets(self):
        for boundary in (study.LOWER,study.UPPER):
            left=study.inventory(D(1000),boundary-D('1e-30'),'hypothetical_wide_0.5_2')
            right=study.inventory(D(1000),boundary+D('1e-30'),'hypothetical_wide_0.5_2')
            self.assertTrue(all(abs(a-b)<D('1e-25') for a,b in zip(left,right)))
        x,y=study.inventory(D(1000),D('.25'),'hypothetical_wide_0.5_2')
        self.assertGreater(x,0);self.assertEqual(y,0)
        x,y=study.inventory(D(1000),D(3),'hypothetical_wide_0.5_2')
        self.assertEqual(x,0);self.assertGreater(y,0)

    def test_cumulative_paid_loss_survives_recovery(self):
        case=study.path_case(D(1000),'full_range','pyusd_down50_then_recover',24)
        final=case['timeline'][-1]
        self.assertEqual(D(final['aggregate_lp_shortfall_usdc_before_position_exits']),0)
        self.assertGreater(D(final['cumulative_paid_reserve_claims_usdc']),0)
        self.assertGreater(D(case['peak_loss_only_reserve_usdc']),D(case['final_paid_shortfall_usdc']))

    def test_linear_exposure_and_concurrent_loss_partition(self):
        small=study.path_case(D(1000),'hypothetical_wide_0.5_2','pyusd_down75_persistent',72)
        large=study.path_case(D(10000),'hypothetical_wide_0.5_2','pyusd_down75_persistent',72)
        self.assertLess(abs(D(large['peak_loss_only_reserve_usdc'])-10*D(small['peak_loss_only_reserve_usdc'])),D('1e-16'))
        for row in small['timeline']:
            total=D(row['two_x_full_initial_gross_counterfactual_principal_collateral_deficit_usdc'])+D(row['two_x_full_initial_gross_counterfactual_owner_shortfall_usdc'])
            self.assertLess(abs(total-D(row['aggregate_lp_shortfall_usdc_before_position_exits'])),D('2e-18'))

    def test_actual_exit_loss_partitions_survive_multiple_price_changes(self):
        case=study.path_case(D(1000),'hypothetical_wide_0.5_2','pyusd_sequential_worsening',24)
        deficit_paid, owner_paid=D(0),D(0)
        for row in case['timeline']:
            deficit_paid+=D(row['two_x_exit_principal_collateral_deficit_usdc'])
            owner_paid+=D(row['two_x_exit_owner_shortfall_usdc'])
            outstanding=D(row['two_x_unexited_principal_collateral_deficit_usdc'])+D(row['two_x_unexited_owner_shortfall_usdc'])
            self.assertLess(abs(deficit_paid+owner_paid+outstanding-D(row['cumulative_required_reserve_usdc'])),D('4e-18'))
        final=case['timeline'][-1]
        self.assertLess(abs(deficit_paid-D('39.64466094')),D('1e-8'))
        self.assertLess(abs(owner_paid-D('296.75457688')),D('1e-8'))
        self.assertEqual(D(final['two_x_unexited_principal_collateral_deficit_usdc']),0)
        self.assertEqual(D(final['two_x_unexited_owner_shortfall_usdc']),0)
        self.assertIn('UNPROVEN',final['two_x_realized_lender_bad_debt'])

    def test_same_supplier_pool_cannot_double_fund_borrowed_capital(self):
        for row in study.study()['capital_purposes_grid']:
            supplier=D(row['supplier_total_capital_usdc'])
            gross=D(row['gross_cl_usdc'])
            gap=D(row['same_supplier_pool_additional_funding_gap_usdc'])
            residual=D(row['same_supplier_pool_cash_after_equity_and_borrow_usdc'])
            self.assertLess(abs(supplier+gap-gross-residual),D('2e-18'))
            if row['leverage_scenario']==2:
                self.assertGreater(gap,0)
                self.assertEqual(residual,0)

    def test_unknown_cost_is_not_zero_and_no_income_credit(self):
        result=study.study()
        self.assertEqual(result['assumptions']['future_income_day1_credit_usdc'],'0')
        for row in result['conditional_seed_summaries']:
            self.assertIsNone(row['conditional_all_in_sensitivity_usdc'])
            self.assertIsNone(row['guaranteed_all_in_seed_usdc'])
            self.assertEqual(row['coverage_status'],'UNPROVEN')

    def test_cost_added_once_before_contingency(self):
        result=study.study(D(100))
        row=result['conditional_seed_summaries'][0]
        loss=D(row['loss_only_floor_usdc_exact'])
        self.assertEqual(row['conditional_all_in_sensitivity_usdc']['0.25'],study.seed_cents((loss+D(100))*D('1.25')))
        with self.assertRaises(ValueError):study.study(D(-1))

    def test_supplier_allocation_does_not_double_count_borrow(self):
        for row in study.study()['capital_purposes_grid']:
            gross=D(row['gross_cl_usdc']);equity=D(row['strategy_owner_equity_usdc'])
            debt=D(row['borrowed_principal_usdc']);supplier=D(row['supplier_total_capital_usdc'])
            self.assertEqual(equity+debt,gross)
            self.assertLess(abs(supplier*D(row['strategy_allocation_fraction_of_supplier_capital'])-equity),D('1e-16'))

    def test_common_depeg_not_insured_as_original_usdc_principal_guarantee(self):
        for model in ('full_range','hypothetical_wide_0.5_2'):
            case=study.path_case(D(1000),model,'both_down25_persistent',72)
            self.assertEqual(D(case['peak_loss_only_reserve_usdc']),0)

    def test_full_outage_duration_and_largest_first_exits(self):
        for hours in (24,72):
            case=study.path_case(D(1000),'full_range','pyusd_down50_persistent',hours)
            exits=[r for r in case['timeline'] if D(r['exit_fraction_initial_gross'])>0]
            self.assertEqual(exits[0]['hour']-case['outage_start_hour'],hours)
            self.assertEqual([D(r['exit_fraction_initial_gross']) for r in exits],list(study.SHARES))


if __name__=='__main__':unittest.main()
