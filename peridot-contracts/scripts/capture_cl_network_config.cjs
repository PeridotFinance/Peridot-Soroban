// Read-only public ledger configuration capture. No accounts, signing, or transactions.
const {rpc,xdr}=require('../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const fs=require('node:fs');
const crypto=require('node:crypto');
async function main(){
 const server=new rpc.Server('https://rpc.lightsail.network/',{timeout:20});
 const network=await server.getNetwork();
 if(network.passphrase!=='Public Global Stellar Network ; September 2015')throw Error('wrong network');
 const ids=['configSettingContractComputeV0','configSettingContractLedgerCostV0','configSettingContractLedgerCostExtV0','configSettingContractHistoricalDataV0','configSettingContractEventsV0','configSettingContractBandwidthV0','configSettingStateArchival','configSettingLiveSorobanStateSizeWindow','configSettingContractCostParamsCpuInstructions','configSettingContractCostParamsMemoryBytes','configSettingContractMaxSizeBytes','configSettingContractDataKeySizeBytes','configSettingContractDataEntrySizeBytes'];
 const keys=ids.map(id=>xdr.LedgerKey.configSetting(new xdr.LedgerKeyConfigSetting({configSettingId:xdr.ConfigSettingId[id]()})));
 const res=await server.getLedgerEntries(...keys);
 if(res.entries.length!==ids.length)throw Error('incomplete network settings');
 const settings={};
 for(const e of res.entries){
   const c=e.val.configSetting();const v=c.value();
   const plain=v=>{if(typeof v==='number'){if(!Number.isSafeInteger(v))throw Error('unsafe integer');return v;}if(Array.isArray(v))return v.map(plain);if(v&&v._attributes)return Object.fromEntries(Object.entries(v._attributes).map(([k,w])=>[k,plain(w)]));if(v&&v.switch)return {variant:v.switch().name||v.switch()};const n=Number(v.toString());if(!Number.isSafeInteger(n))throw Error('unsafe config integer');return n;};
   const values=plain(v);
   settings[c.switch().name]={values,xdr:e.val.toXDR('base64'),lastModifiedLedger:e.lastModifiedLedgerSeq};
 }
 const report={scope:'read-only Mainnet fee/limit settings; test execution remains local controlled state',rpc:'https://rpc.lightsail.network/',network,ledger:res.latestLedger,settings};
 const data=JSON.stringify(report,null,2)+'\n';
 const output=process.argv[2]||'target/cl-next/network-config.json';
 fs.writeFileSync(output,data);
 console.log(JSON.stringify({ledger:res.latestLedger,protocol:network.protocolVersion,sha256:crypto.createHash('sha256').update(data).digest('hex'),settings:Object.keys(settings)}));
}
main().catch(e=>{console.error(e.message);process.exitCode=1});
