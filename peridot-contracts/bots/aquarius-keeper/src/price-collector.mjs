import { POLICY,windowPrice } from './observation.mjs';
export async function jsonGet(url) {
  const response=await fetch(url,{signal:AbortSignal.timeout(15_000),redirect:'error'});
  if(!response.ok)throw Error(`price data HTTP ${response.status}`);
  return response.json();
}
export async function collectWindow({horizonUrl,networkPassphrase,code,issuer,now,get=jsonGet}) {
  const root=new URL(horizonUrl);
  if(root.protocol!=='https:' || root.username || root.password || root.search || root.hash)throw Error('invalid Horizon origin');
  const metadata=await get(root);
  if(metadata.network_passphrase!==networkPassphrase)throw Error('Horizon network mismatch');
  const ledgerUrl=new URL('/ledgers',root);ledgerUrl.search=new URLSearchParams({order:'desc',limit:'1'});
  const ledgers=await get(ledgerUrl),last=ledgers?._embedded?.records?.[0];
  const close=Date.parse(last?.closed_at)/1000;
  if(!Number.isSafeInteger(close) || close>now() || now()-close>60)throw Error('Horizon ledger stale/future');
  const end=Math.floor(now()/POLICY.bucketSeconds)*POLICY.bucketSeconds;
  if(close<end)throw Error('Horizon has not indexed the complete window');
  const url=new URL('/trade_aggregations',root);
  url.search=new URLSearchParams({base_asset_type:code.length<=4?'credit_alphanum4':'credit_alphanum12',base_asset_code:code,
    base_asset_issuer:issuer,counter_asset_type:'native',start_time:String((end-POLICY.windowSeconds)*1000),
    end_time:String(end*1000),resolution:String(POLICY.bucketSeconds*1000),order:'asc',limit:'200'});
  const page=await get(url);
  // One bounded page must contain precisely all six expected CLOSED buckets.
  // Extra records, a truncated response or a missing bucket are rejected.
  return windowPrice(page?._embedded?.records,end,now());
}
