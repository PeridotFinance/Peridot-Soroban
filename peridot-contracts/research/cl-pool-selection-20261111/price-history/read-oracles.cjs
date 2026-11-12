// PUBLIC getters only. No wallet, signing, submission or environment access.
const S=require('../../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const fs=require('node:fs'),path=require('node:path');
const endpoint='https://rpc.lightsail.network/',server=new S.rpc.Server(endpoint,{timeout:30});
const source='GBNZILSTVQZ4R7IKQDGHYGY2QXL5QOFJYQMXPKWRRM5PAV7Y4M67AQUA';
const external='CAFJZQWSED6YAWZU3GWRTOCNPPCGBN32L7QV43XX5LZLFTK6JLN34DLN',dex='CALI2BYU2JE6WVRUFYTS6MSBNEHGJ35P4AVCZYF3B6QOE3QKOB2PLE6M',forex='CBKGPWGKSKZF52CFHMTRR23TBWTPMRDIYZ4O2P5VS65BMHYH4DXMCJZC';
const contracts={PYUSD:'CCCRWH6Q3FNP3I2I57BDLM5AFAT7O6OF6GKQOC6SSJNDAVRZ57SPHGU2',sUSD:'CDFZUVS5YNLXU7VENKOUDEOHCJGKQNVUBWD7KMN6E7ZROKPYPFLRUJFG',USDT0:'CBSJZEIO5C7KC2SF3MKSNXXJSW5G3VTNBX4ATMKUI3B2MR4JKM4R26YF',EURC:'CDTKPWPLOURQA2SGTKTUQOWRCBZEORB4BWBOMJ3D3ZTQQSGE5F6JBQLV',USDC:'CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75'};
const json=x=>JSON.stringify(x,(_,v)=>typeof v==='bigint'?v.toString():v,2);
const out={endpoint,retrievedAt:new Date().toISOString(),scope:'independent unsigned public getter simulations; not financial roots; declaration CPU not metered execution CPU; no signatures',views:[]};
const save=()=>fs.writeFileSync(path.join(__dirname,'oracles.json'),json(out)+'\n');
(async()=>{out.network=await server.getNetwork();if(out.network.passphrase!==S.Networks.PUBLIC)throw Error('wrongnetwork');const acct=await server.getAccount(source);
async function view(id,method,args=[],label=method){const row={contract:id,method,label};try{const tx=new S.TransactionBuilder(acct,{fee:'100',networkPassphrase:S.Networks.PUBLIC}).addOperation(new S.Contract(id).call(method,...args)).setTimeout(60).build();const sim=await server.simulateTransaction(tx);row.ledger=sim.latestLedger;row.cost=sim.cost??null;if(S.rpc.Api.isSimulationSuccess(sim)){row.value=S.scValToNative(sim.result.retval);row.returnXdr=sim.result.retval.toXDR('base64');row.minResourceFee=sim.minResourceFee;row.requiresRestore=!!sim.restorePreamble;const data=sim.transactionData.build(),r=data.resources(),fp=r.footprint();row.resourceDeclaration={instructions:r.instructions(),diskReadBytes:r.diskReadBytes?r.diskReadBytes():r.readBytes(),writeBytes:r.writeBytes(),uniqueKeys:fp.readOnly().length+fp.readWrite().length,writes:fp.readWrite().length};row.transactionDataXdr=data.toXDR('base64');row.keys={ro:fp.readOnly().map(x=>x.toXDR('base64')),rw:fp.readWrite().map(x=>x.toXDR('base64'))};}else row.error=sim.error;}catch(e){row.error=e.message;}out.views.push(row);save();}
for(const id of[external,dex,forex])for(const method of['assets','base','decimals','resolution','history_retention_period'])await view(id,method);
for(const code of['USDC','EURC','PYUSD','sUSD','USDT0'])await view(external,'lastprice',[S.xdr.ScVal.scvVec([S.xdr.ScVal.scvSymbol('Other'),S.xdr.ScVal.scvSymbol(code)])],`external-${code}`);
for(const[code,id]of Object.entries(contracts))await view(dex,'lastprice',[S.xdr.ScVal.scvVec([S.xdr.ScVal.scvSymbol('Stellar'),new S.Address(id).toScVal()])],`dex-${code}`);
await view(forex,'lastprice',[S.xdr.ScVal.scvVec([S.xdr.ScVal.scvSymbol('Other'),S.xdr.ScVal.scvSymbol('EUR')])],'forex-EUR');
console.log(json(out.views.map(({returnXdr,keys,transactionDataXdr,...x})=>x)));
})().catch(e=>{out.error=e.message;save();console.error(e);process.exitCode=1;});
