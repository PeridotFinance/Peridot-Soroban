// Public unsigned estimates only; no key, signing or submission capability.
const S=require('../../../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk'),fs=require('fs'),path=require('path');
const p=__dirname,pool=JSON.parse(fs.readFileSync(path.join(p,'../apy-boost/detail.json'))),chain=JSON.parse(fs.readFileSync(path.join(p,'../apy-boost/getters.json'))).pools[0];
const tokens=JSON.parse(fs.readFileSync(path.join(p,'tokens-live.json'))).items;
const u=tokens.find(t=>t.address==='CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75');
const prices=pool.tokens_addresses.map(a=>Number(tokens.find(t=>t.address===a).price_xlm)/Number(u.price_xlm));
const slot=chain.views.find(v=>v.method==='get_slot0').value,spacing=pool.tick_spacing,lo=Math.floor((slot.tick-2000)/spacing)*spacing,hi=Math.ceil((slot.tick+2000)/spacing)*spacing;
const sp=Number(slot.sqrt_price_x96)/2**96,a=1.0001**(lo/2),b=1.0001**(hi/2),c0=(b-sp)/(sp*b),c1=sp-a,cap=10000,L=cap*1e7/(c0*prices[0]+c1*prices[1]);
const amounts=[BigInt(Math.floor(L*c0)),BigInt(Math.floor(L*c1))];
const out={at:new Date().toISOString(),scope:'independent rolling unsigned live estimates; no executed deposit, no sequential path proof',pool:pool.address,range:[lo,hi],capital:cap,prices,slot,desired_amounts:amounts,views:[]};
const dest=path.join(p,'quotes.json');if(fs.existsSync(dest))throw Error('refuse evidence overwrite');
const json=x=>JSON.stringify(x,(_,v)=>typeof v==='bigint'?v.toString():v,2),save=()=>fs.writeFileSync(dest,json(out)+'\n');
const server=new S.rpc.Server('https://rpc.lightsail.network/',{timeout:30});
(async()=>{const net=await server.getNetwork();if(net.passphrase!==S.Networks.PUBLIC)throw Error('wrongnetwork');const acct=await server.getAccount('GBNZILSTVQZ4R7IKQDGHYGY2QXL5QOFJYQMXPKWRRM5PAV7Y4M67AQUA');
async function view(label,method,args,extra={}){await new Promise(r=>setTimeout(r,600));const row={label,method,...extra};try{const tx=new S.TransactionBuilder(acct,{fee:'100',networkPassphrase:S.Networks.PUBLIC}).addOperation(new S.Contract(pool.address).call(method,...args)).setTimeout(60).build();const sim=await server.simulateTransaction(tx);row.ledger=sim.latestLedger;if(S.rpc.Api.isSimulationSuccess(sim)){row.value=S.scValToNative(sim.result.retval);row.returnXdr=sim.result.retval.toXDR('base64');row.resourceFee=sim.minResourceFee;}else row.error=sim.error;}catch(e){row.error=e.message;}out.views.push(row);save();return row;}
await view('deposit','estimate_deposit_position',[S.nativeToScVal(lo,{type:'i32'}),S.nativeToScVal(hi,{type:'i32'}),S.xdr.ScVal.scvVec(amounts.map(x=>S.nativeToScVal(x,{type:'u128'})))]);
for(const leg of[0,1]){const initial=BigInt(Math.floor(cap/prices[leg]*1e7)),keep=amounts[leg],input=initial-keep;await view('entry_'+leg,'estimate_swap',[S.nativeToScVal(leg,{type:'u32'}),S.nativeToScVal(1-leg,{type:'u32'}),S.nativeToScVal(input,{type:'u128'})],{leg,initial,keep,input});}
console.log(json(out));})().catch(e=>{out.error=e.message;save();console.error(e);process.exitCode=1;});
