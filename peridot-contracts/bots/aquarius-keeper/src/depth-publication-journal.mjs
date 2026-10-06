// Append-only PUBLIC transaction metadata. Never stores keys or signed envelopes.
// A stale lock or partial record requires operator reconciliation, not auto-retry.
import {open,unlink} from 'node:fs/promises';
import {dirname,isAbsolute} from 'node:path';
import assert from 'node:assert/strict';
import {Networks,TransactionBuilder} from '@stellar/stellar-sdk';
const hashPattern=/^[a-f0-9]{64}$/;
const methods=['publish_observation','publish_recovery_observation','invalidate_observation'];
const sequence=v=>typeof v==='string'&&/^[1-9][0-9]{0,18}$/.test(v)&&BigInt(v)<=9223372036854775807n;
function details(v) {
  assert(v&&/^G[A-Z2-7]{55}$/.test(v.source)&&sequence(v.sequence)
    &&Number.isSafeInteger(v.maxTime)&&v.maxTime>0,'invalid intent details');
  return {source:v.source,sequence:v.sequence,maxTime:v.maxTime};
}
function fenceRecord(v) {
  assert(v&&hashPattern.test(v.hash)&&typeof v.unsignedXdr==='string'&&v.unsignedXdr.length<4096
    &&sequence(v.bumpTo),'invalid fence');
  assert.equal(TransactionBuilder.fromXDR(v.unsignedXdr,Networks.TESTNET).signatures.length,0,'signed envelope forbidden');
  return {hash:v.hash,unsignedXdr:v.unsignedXdr,bumpTo:v.bumpTo};
}
export async function openPublicationJournal(path,scope) {
  assert(isAbsolute(path)&&hashPattern.test(scope),'invalid journal configuration');
  const lock=await open(`${path}.lock`,'wx',0o600);
  let file,closed=false,pending=null,last=null,size=0,fence=null;
  try {
    await lock.sync();
    file=await open(path,'a+',0o600);
    const directory=await open(dirname(path),'r');
    try {await directory.sync();}finally {await directory.close();}
    const stat=await file.stat();
    assert(stat.isFile()&&stat.size<=1_048_576,'journal size/type guard');
    size=stat.size;
    const contents=await file.readFile('utf8');
    assert(!contents||contents.endsWith('\n'),'partial journal record');
    for(const line of contents.split('\n').filter(Boolean)) {
      const r=JSON.parse(line);
      assert(r.scope===scope&&hashPattern.test(r.hash),'journal scope/hash mismatch');
      if(r.state==='intent') {
        assert(!pending&&methods.includes(r.method),'invalid journal intent');
        if(r.details)details(r.details);
        pending=r;
      } else if(r.state==='fence_intent') {
        assert(pending&&r.hash===pending.hash&&!fence,'invalid recovery fence');
        fence=fenceRecord(r.fence);
      } else if(r.state==='RETIRED') {
        assert(pending?.hash===r.hash&&fence?.hash===r.fenceHash,'invalid retirement');
        pending=null;fence=null;
      } else {
        assert(!fence&&['SUCCESS','FAILED'].includes(r.state)&&pending?.hash===r.hash,'invalid journal resolution');
        pending=null;
      }
      last=r;
    }
  } catch(error) {
    await file?.close();await lock.close();await unlink(`${path}.lock`);throw error;
  }
  const append=async record=>{
    assert(!closed,'journal closed');
    const line=JSON.stringify(record)+'\n';
    assert(size+Buffer.byteLength(line)<=1_048_576,'journal full: reconcile and archive');
    await file.writeFile(line);await file.sync();size+=Buffer.byteLength(line);last=record;
  };
  return {
    pending:()=>pending?{...pending}:null,
    fence:()=>fence?{...fence}:null,
    failed:()=>last?.state==='FAILED',
    async intent(hash,method,metadata) {
      assert(!pending&&hashPattern.test(hash)&&methods.includes(method),'invalid intent');
      const record={scope,state:'intent',hash,method,...(metadata?{details:details(metadata)}:{})};
      // Reserve before IO: any partial/failed durable write poisons this process.
      pending=record;await append(record);
    },
    async resolve(hash,status) {
      assert(!fence&&pending?.hash===hash&&['SUCCESS','FAILED'].includes(status),'invalid resolution');
      await append({scope,state:status,hash});pending=null;
    },
    async fenceIntent(hash,value) {
      assert(pending?.hash===hash&&!fence,'invalid recovery intent');
      fence=fenceRecord(value);
      await append({scope,state:'fence_intent',hash,fence});
    },
    async retire(hash,fenceHash) {
      assert(pending?.hash===hash&&fence?.hash===fenceHash,'invalid retirement');
      await append({scope,state:'RETIRED',hash,fenceHash});pending=null;fence=null;
    },
    async close() {
      if(closed)return;closed=true;
      await file.close();await lock.close();await unlink(`${path}.lock`);
    }
  };
}
