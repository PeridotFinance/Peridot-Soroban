// Fixed diagnostic vocabulary only. Never serialize SDK errors, URLs, headers,
// subprocess stderr or environment content. These labels cannot approve prices.
const stages=new Set(['depth','aquarius','cross_check','subprocess','validation']);
const reasons=new Set(['transport_timeout','transport_failure','http_rate_limit','http_server_error',
  'http_client_error','stale_ledger','missing_direct_route','spread_guard','depth_impact_guard',
  'ratio_guard','ledger_coherence_guard','collection_too_slow','quote_unavailable',
  'invalid_response','guard_failed','process_timeout','process_failed']);
const messages=new Map([
  ['stale/future ledger','stale_ledger'],['one unambiguous direct route required','missing_direct_route'],
  ['shadow spread exceeds 1%','spread_guard'],['depth impact exceeds 0.5%','depth_impact_guard'],
  ['shadow ratio bounds','ratio_guard'],['incoherent ledger sample','ledger_coherence_guard'],
  ['incoherent RPC quote sample','ledger_coherence_guard'],['Horizon/RPC ledger mismatch','ledger_coherence_guard'],
  ['quote collection too slow','collection_too_slow'],['shadow sample expired during cross-check','collection_too_slow'],
  ['get_tokens unavailable/restoration required','quote_unavailable'],
  ['estimate_swap unavailable/restoration required','quote_unavailable'],
]);
export function safeFailure(value) {
  return stages.has(value?.stage)&&reasons.has(value?.reason)
    ? {stage:value.stage,reason:value.reason}:null;
}
export function classifyFailure(error,stage) {
  let reason='guard_failed';
  const status=Number.isInteger(error?.response?.status)?error.response.status
    : /^price data HTTP [1-5][0-9]{2}$/.test(error?.message??'')?Number(error.message.slice(-3)):0;
  if(status===429)reason='http_rate_limit';
  else if(status>=500&&status<=599)reason='http_server_error';
  else if(status>=400&&status<=499)reason='http_client_error';
  else if(error?.name==='TimeoutError'||['ETIMEDOUT','ECONNABORTED'].includes(error?.code))reason='transport_timeout';
  else if(error?.message==='fetch failed'||['ECONNRESET','ENOTFOUND','EAI_AGAIN'].includes(error?.code))reason='transport_failure';
  else if(error instanceof SyntaxError)reason='invalid_response';
  else reason=messages.get(error?.message)??reason;
  return safeFailure({stage,reason})??{stage:'validation',reason:'guard_failed'};
}
export function collectorOutput(stdout) {
  let value;
  try {value=JSON.parse(stdout);}catch {throw diagnosticError({stage:'subprocess',reason:'invalid_response'});}
  if(value?.kind==='observer_collection_error')throw diagnosticError(safeFailure(value)??{stage:'subprocess',reason:'process_failed'});
  return value; // caller must still validate every sample, including freshness.
}
export function diagnosticError(diagnostic) {
  const error=new Error('observer collection failed');
  error.diagnostic=safeFailure(diagnostic);
  return error;
}
