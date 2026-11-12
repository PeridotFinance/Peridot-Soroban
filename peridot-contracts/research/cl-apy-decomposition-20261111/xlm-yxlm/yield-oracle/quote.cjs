// Public getters/estimates only. No wallets, secret/environment access, signing or submission.
const S=require('../../../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const fs=require('node:fs'),path=require('node:path');
const endpoint='https://rpc.lightsail.network/',server=new S.rpc.Server(endpoint,{timeout:30});
const source='GBNZILSTVQZ4R7IKQDGHYGY2QXL5QOFJYQMXPKWRRM5PAV7Y4M67AQUA';
const external='CAFJZQWSED6YAWZU3GWRTOCNPPCGBN32L7QV43XX5LZLFTK6JLN34DLN',dex='CALI2BYU2JE6WVRUFYTS6MSBNEHGJ35P4AVCZYF3B6QOE3QKOB2PLE6M',forex='CBKGPWGKSKZF52CFHMTRR23TBWTPMRDIYZ4O2P5VS65BMHYH4DXMCJZC';
const eurc='CDTKPWPLOURQA2SGTKTUQOWRCBZEORB4BWBOMJ3D3ZTQQSGE5F6JBQLV',usdc='CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75';
const cp='CA6GAFOJCW4MGQQBUCQUSA3CLIH25G4SNKB2JHYKZCVWZTNW5VXMSC4O',eurpool='CDTSE6RLRI7ZO25JSER6E4SQR4PHJJNONEGS5HDJ3Y6LAKECRZKYN5CA';
const json=x=>JSON.stringify(x,(_,v)=>typeof v==='bigint'?v.toString():v,2);
const out={endpoint,retrievedAt:new Date().toISOString(),scope:'unsigned public read-only getter/estimate simulations; rolling state; not a sale or financial execution',views:[]};
const dest=path.join(__dirname,process.argv[2]||'reads.json');if(fs.existsSync(dest))throw Error('refuse overwrite evidence');const save=()=>fs.writeFileSync(dest,json(out)+'\n');
(async()=>{out.network=await server.getNetwork();if(out.network.passphrase!==S.Networks.PUBLIC)throw Error('wrongnetwork');const acct=await server.getAccount(source);
async function view(id,method,args=[],label=method){await new Promise(r=>setTimeout(r,500));const row={contract:id,method,label,args:args.map(x=>x.toXDR('base64')),time:new Date().toISOString()};try{const tx=new S.TransactionBuilder(acct,{fee:'100',networkPassphrase:S.Networks.PUBLIC}).addOperation(new S.Contract(id).call(method,...args)).setTimeout(60).build();const sim=await server.simulateTransaction(tx);row.ledger=sim.latestLedger;row.cost=sim.cost??null;if(S.rpc.Api.isSimulationSuccess(sim)){row.value=S.scValToNative(sim.result.retval);row.returnXdr=sim.result.retval.toXDR('base64');row.minResourceFee=sim.minResourceFee;row.requiresRestore=!!sim.restorePreamble;const data=sim.transactionData.build(),r=data.resources(),fp=r.footprint();row.resourceDeclaration={instructions:r.instructions(),diskReadBytes:r.diskReadBytes?r.diskReadBytes():r.readBytes(),writeBytes:r.writeBytes(),uniqueKeys:fp.readOnly().length+fp.readWrite().length,writes:fp.readWrite().length};row.transactionDataXdr=data.toXDR('base64');}else row.error=sim.error;}catch(e){row.error=e.message;}out.views.push(row);save();}
if(!process.argv[3]){
for(const id of[external,dex,forex])for(const method of['base','decimals','resolution','last_timestamp'])await view(id,method,[],id.slice(0,8)+'-'+method);
for(const code of['USDC','EURC'])await view(external,'lastprice',[S.xdr.ScVal.scvVec([S.xdr.ScVal.scvSymbol('Other'),S.xdr.ScVal.scvSymbol(code)])],`external-${code}`);
await view(dex,'lastprice',[S.xdr.ScVal.scvVec([S.xdr.ScVal.scvSymbol('Stellar'),new S.Address(eurc).toScVal()])],'dex-EURC');
await view(forex,'lastprice',[S.xdr.ScVal.scvVec([S.xdr.ScVal.scvSymbol('Other'),S.xdr.ScVal.scvSymbol('EUR')])],'forex-EUR');
for(const method of['get_slot0','get_tokens','get_fee_fraction'])await view(eurpool,method,[],'EURCpool-'+method);
for(const method of['get_tokens','get_reserves','get_fee_fraction','get_protocol_fee_fraction'])await view(cp,method,[],'AQUApool-'+method);
}
const amounts=JSON.parse(fs.readFileSync(path.join(__dirname,process.argv[3]||'amounts.json')));
for(const [label,raw]of Object.entries(amounts))await view(cp,'estimate_swap',[S.nativeToScVal(0,{type:'u32'}),S.nativeToScVal(1,{type:'u32'}),S.nativeToScVal(BigInt(raw),{type:'u128'})],label);
console.log(json(out.views.map(({returnXdr,transactionDataXdr,args,...x})=>x)));
})().catch(e=>{out.error=e.message;save();console.error(e);process.exitCode=1;});
