// Append-only PUBLIC transaction metadata. Never stores keys or signed envelopes.
// A stale lock or partial record requires operator reconciliation, not auto-retry.
import {open,unlink} from 'node:fs/promises';
import {dirname,isAbsolute} from 'node:path';
import assert from 'node:assert/strict';
const hashPattern=/^[a-f0-9]{64}$/;
export async function openPublicationJournal(path,scope) {
  assert(isAbsolute(path)&&hashPattern.test(scope),'invalid journal configuration');
  const lock=await open(`${path}.lock`,'wx',0o600);
  let file,closed=false,pending=null,last=null,size=0;
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
        assert(!pending&&['publish_observation','invalidate_observation'].includes(r.method),'invalid journal intent');
        pending=r;
      } else {
        assert(['SUCCESS','FAILED'].includes(r.state)&&pending?.hash===r.hash,'invalid journal resolution');
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
    failed:()=>last?.state==='FAILED',
    async intent(hash,method) {
      assert(!pending&&hashPattern.test(hash)&&['publish_observation','invalidate_observation'].includes(method),'invalid intent');
      const record={scope,state:'intent',hash,method};
      // Reserve before IO: any partial/failed durable write poisons this process.
      pending=record;await append(record);
    },
    async resolve(hash,status) {
      assert(pending?.hash===hash&&['SUCCESS','FAILED'].includes(status),'invalid resolution');
      await append({scope,state:status,hash});pending=null;
    },
    async close() {
      if(closed)return;closed=true;
      await file.close();await lock.close();await unlink(`${path}.lock`);
    }
  };
}
