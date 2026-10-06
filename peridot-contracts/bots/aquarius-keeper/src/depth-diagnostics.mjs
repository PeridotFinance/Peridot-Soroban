// Only fixed stage labels and decoded protocol enum names may leave this module.
export function depthDiagnostic(stage,error) {
  const out={kind:'depth_stage_failed',stage,network:'testnet'};
  try {
    const code=error?.errorResult?.result().switch().name;
    if(typeof code==='string'&&/^tx[A-Za-z]+$/.test(code)&&code.length<80)out.resultCode=code;
  }catch { /* Never stringify SDK exceptions or XDR. */ }
  return out;
}
export async function depthStage(emit,stage,fn) {
  try {return await fn();}catch(error){emit(depthDiagnostic(stage,error));throw Error(`depth stage failed: ${stage}`);}
}
