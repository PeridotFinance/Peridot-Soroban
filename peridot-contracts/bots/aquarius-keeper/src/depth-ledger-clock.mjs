import assert from 'node:assert/strict';
import {setTimeout as sleep} from 'node:timers/promises';

// RPC can return a just-closed ledger a few seconds ahead of this machine.
// Wait for its ACTUAL timestamp; never accept a future ledger or widen max age.
export async function requireClosedLedger(ledger,now,wait=sleep) {
  const timestamp=Number(ledger.closeTime),current=now();
  assert(Number.isSafeInteger(current)&&Number.isSafeInteger(timestamp)&&timestamp>0,'invalid RPC clock');
  const ahead=timestamp-current;
  assert(ahead<=5,'future RPC ledger');
  if(ahead>0)await wait(ahead*1000);
  const after=now();
  assert(Number.isSafeInteger(after)&&after>=current&&timestamp<=after&&after-timestamp<=60,'stale/future RPC ledger');
}
