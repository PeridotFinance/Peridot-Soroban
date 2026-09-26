import test from 'node:test';
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
const read=path=>readFile(new URL(path,import.meta.url),'utf8');
test('Testnet image pins base and copies only reporter dependencies',async()=>{
  const docker=await read('../Dockerfile.depth-testnet');
  assert.match(docker,/^FROM node:22-alpine@sha256:[a-f0-9]{64}$/m);
  assert.match(docker,/RUN npm ci --omit=dev --ignore-scripts/);
  assert.match(docker,/^USER node$/m);
  assert.match(docker,/CMD \["node", "src\/depth-publisher-main.mjs"\]/);
  const copies=docker.split('\n').filter(x=>x.startsWith('COPY '));
  assert.equal(copies.length,2);
  assert(!copies.join(' ').match(/(?:\.env|target\/|scripts\/|src\/main\.mjs|\*|COPY \. )/));
  const sourceFiles=copies[1].split(' ').slice(1,-1);
  assert.equal(sourceFiles.length,14);
  for(const path of sourceFiles)assert.match(path,/^src\/[a-z-]+\.mjs$/);
});
test('Testnet service preserves state and does not auto-retry or expose key arguments',async()=>{
  const service=await read('../deploy/peridot-depth-testnet.service');
  for(const value of ['Restart=no','--read-only','--user 1000:1000','--cap-drop ALL',
    '--security-opt no-new-privileges','--log-driver none','--env CONFIRM_DEPTH_TESTNET=ISOLATED_DEPTH_REPLAY',
    '--env DEPTH_TESTNET_JOURNAL=/state/publisher.jsonl',
    'src=/var/lib/peridot-depth-testnet,dst=/state','--env DEPTH_TESTNET_REPORTER_SECRET '])assert(service.includes(value));
  assert(!service.includes('DEPTH_TESTNET_REPORTER_SECRET='));
  assert(!service.match(/ExecStartPre=|Restart=always|--privileged|docker\.sock|ADMIN_SECRET|MAINNET/));
});
