// Proposed replacement policy. Pure report/planning functions: no keys, network
// writes or authority to activate borrowing. On-chain guards remain mandatory.
import {createHash} from 'node:crypto';
import {assessWindow} from './price-shadow-soak.mjs';
import {near} from './observation.mjs';
export const DEPTH_PRICING_POLICY=Object.freeze({id:'yxlm-depth-twap-candidate-v1',
  windowSeconds:1800,minimumUpdateSeconds:300,reportMaxAgeSeconds:60,
  onChainMaxAgeSeconds:300,maxStepBps:100n,minRatio:800000000000n,maxRatio:1050000000000n});
const hold=reason=>({state:'unavailable',reason,publicationEligible:false});
export function depthCandidate(records,now,startedAt) {
  try {
    if(!Number.isSafeInteger(now)||!Number.isSafeInteger(startedAt)||startedAt<=0||now<startedAt)
      return hold('invalid_process_clock');
    if(!Array.isArray(records)||records.length>64)return hold('invalid_history');
    if(now-startedAt<1800)return {state:'warming',publicationEligible:false};
    // Recompute from raw samples; a precomputed healthy/agrees flag is not proof.
    const w=assessWindow(records,now);
    if(w.state!=='healthy')return hold(w.reason??'incomplete_window');
    if(w.start<startedAt)return {state:'warming',publicationEligible:false};
    let first=-1;
    for(let i=0;i<records.length;i++)if(records[i].state==='ok'&&records[i].sample.point.timestamp<=w.start)first=i;
    const selected=records.slice(first);
    if(first<0||selected.some((r,i)=>!Number.isSafeInteger(r.startedAt)||!Number.isSafeInteger(r.finishedAt)
      ||r.startedAt<startedAt||r.finishedAt<r.startedAt||r.finishedAt>now
      ||r.finishedAt-r.startedAt>45||!Number.isSafeInteger(r.index)
      ||(i>0&&(r.index!==selected[i-1].index+1||r.startedAt<=selected[i-1].finishedAt))))
      return hold('invalid_attempt_history');
    const ratio=BigInt(w.ratio);
    if(ratio<DEPTH_PRICING_POLICY.minRatio||ratio>DEPTH_PRICING_POLICY.maxRatio)return hold('ratio_bounds');
    const digest=createHash('sha256').update(JSON.stringify(selected.map(r=>({index:r.index,
      startedAt:r.startedAt,finishedAt:r.finishedAt,point:r.sample.point,aquarius:r.sample.aquarius})))).digest('hex');
    return {state:'candidate',publicationEligible:false,policy:DEPTH_PRICING_POLICY.id,
      ratio:ratio.toString(),start:w.start,end:w.end,samples:selected.length,evidenceSha256:digest};
  }catch {return hold('candidate_guard_failed');}
}

// A proposed action, not a submitted transaction. `previous` must be freshly read
// from get_observation; never substitute the last locally proposed report.
export function planDepthPublication({records,now,startedAt,previous}) {
  const candidate=depthCandidate(records,now,startedAt);
  const result=(action,reason)=>({action,reason,candidate,publicationEligible:false});
  if(!Number.isSafeInteger(now)||now<=0)return result('halt','invalid_process_clock');
  if(!previous||typeof previous.valid!=='boolean'
    ||['ratio','start','end','invalidated_at'].some(k=>typeof previous[k]!=='bigint'||previous[k]<0n)
    ||previous.end>BigInt(now)||previous.invalidated_at>BigInt(now)
    ||(previous.valid&&previous.end===0n)
    ||(previous.end>0n&&(previous.end-previous.start!==1800n
      ||previous.ratio<DEPTH_PRICING_POLICY.minRatio||previous.ratio>DEPTH_PRICING_POLICY.maxRatio)))
    return result('halt','invalid_chain_state');
  const reject=reason=>result(previous?.valid?'invalidate':'hold',reason);
  if(candidate.state!=='candidate')return reject(candidate.reason??'fresh_window_required');
  const end=BigInt(candidate.end),ratio=BigInt(candidate.ratio);
  if(now-candidate.end>DEPTH_PRICING_POLICY.reportMaxAgeSeconds)return reject('candidate_expired');
  if(previous&&(end<=previous.end||end<=previous.invalidated_at))return result('hold','newer_window_required');
  if(previous?.end>0n){
    // Step violations invalidate even when too early to publish; do not silently
    // retain a live old observation through a newly detected large move.
    if(!near(ratio,previous.ratio,DEPTH_PRICING_POLICY.maxStepBps))return reject('ratio_step');
    if(end-previous.end<BigInt(DEPTH_PRICING_POLICY.minimumUpdateSeconds))return result('hold','minimum_interval');
  }
  return result('publish_candidate','review_and_live_crosscheck_required');
}
