"""Offline public-plan validation; no deployment or signing."""
import copy
import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('funding', ROOT / 'scripts/prepare_funding_group.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

class FundingPlanTests(unittest.TestCase):
    def config(self):
        market = dict(cf_scaled=800000,supply_cap=1000000,borrow_cap=500000,
                      target_cash_bps=4000,min_cash_reserve_bps=1000,
                      rate_model=dict(base=0,multiplier=100000,jump=1000000,kink=800000))
        return dict(admin='G'+'A'*55,oracle='C'+'A'*55,
                    markets={'XLM':copy.deepcopy(market),'USDC':copy.deepcopy(market)})
    def test_real_abi_and_durable_borrow_pause(self):
        plan=module.prepare(self.config())
        self.assertEqual(plan['status'],'PREPARED_NOT_DEPLOYED')
        self.assertFalse(plan['network_actions'])
        self.assertFalse(plan['borrowing_enabled'])
        self.assertIsNone(plan['maintenance_margin'])
        self.assertEqual(plan['strategies'],[])
        calls=[a for a in plan['actions'] if 'method' in a]
        for asset in ('XLM','USDC'):
            enabled=[a for a in calls if a['target']==asset and a['method']=='set_cash_borrowing_enabled']
            self.assertEqual([a['args']['enabled'] for a in enabled],[False])
            init_rate=next(a for a in calls if a['target']==asset+'_rate')
            self.assertEqual(set(init_rate['args']),{'admin','base','multiplier','jump','kink'})
        self.assertFalse(any('upshift' in str(a).lower() or a['method']=='set_boosted_vault' for a in calls))
        sources={
            'controller':(ROOT/'contracts/simple-peridottroller/src/contract.rs').read_text(),
            'rate':(ROOT/'contracts/jump-rate-model/src/lib.rs').read_text(),
            'receipt':(ROOT/'contracts/receipt-vault/src/contract.rs').read_text(),
        }
        for a in calls+plan['required_readbacks']:
            source=sources['controller' if a['target']=='controller' else 'rate' if a['target'].endswith('_rate') else 'receipt']
            self.assertIn('pub fn '+a['method']+'(',source,a)
    def test_reject_unset_or_unsafe_configuration(self):
        for field,value in [('maintenance_margin',250000),('upshift',None),('secret','not-a-key')]:
            c=self.config(); c[field]=value
            with self.assertRaises(ValueError): module.prepare(c)
        for key,value in [('supply_cap',0),('borrow_cap',0),('cf_scaled',950000),('min_cash_reserve_bps',5000),('rate_model',{})]:
            c=self.config(); c['markets']['XLM'][key]=value
            with self.assertRaises(ValueError): module.prepare(c)

if __name__=='__main__': unittest.main()
