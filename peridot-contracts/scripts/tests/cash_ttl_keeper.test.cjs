const test=require('node:test'),assert=require('node:assert/strict');
const S=require('../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const {validate,SOURCE,EXTEND}=require('../cash_ttl_keeper.cjs');
const key=S.xdr.LedgerKey.contractCode(new S.xdr.LedgerKeyContractCode({hash:Buffer.from('847894a65fb59675c74707ea8899633a7638aa2716a183b0b2efe35a8bc2b902','hex')}));
function tx(o={}){const data=new S.SorobanDataBuilder().setReadOnly(o.ro||[key]).setReadWrite(o.rw||[]).build();const b=new S.TransactionBuilder(new S.Account(o.source||SOURCE,'1'),{fee:o.fee||'100',networkPassphrase:o.network||S.Networks.PUBLIC,sorobanData:data}).addOperation(o.operation||S.Operation.extendFootprintTtl({extendTo:o.extendTo||EXTEND}));if(o.second)b.addOperation(S.Operation.extendFootprintTtl({extendTo:EXTEND}));return b.setTimeout(300).build();}
const keys=[key.toXDR('base64')];
test('allows only the pinned native TTL shape',()=>assert.doesNotThrow(()=>validate(tx(),keys)));
test('rejects Ledger source',()=>assert.throws(()=>validate(tx({source:'GC4T5MP5ZKILC633IUL6GHWGSIUV6JREN7BKOM6X7XTSVYLNKWZIT7FP'}),keys)));
test('rejects financial invocation',()=>assert.throws(()=>validate(tx({operation:new S.Contract('CDW3GDNKLUEPPJLEUVGLJSKGYLB3YYDQBUY7NBHJILGQWNN55AYFHY3F').call('deposit',new S.Address(SOURCE).toScVal(),S.nativeToScVal(1n,{type:'u128'}))}),keys)));
test('rejects unapproved code',()=>{const bad=S.xdr.LedgerKey.contractCode(new S.xdr.LedgerKeyContractCode({hash:Buffer.alloc(32)}));assert.throws(()=>validate(tx({ro:[bad]}),[bad.toXDR('base64')]));});
test('rejects read-write footprint',()=>assert.throws(()=>validate(tx({rw:[key]}),keys)));
test('rejects substituted footprint',()=>assert.throws(()=>validate(tx(),[])));
test('rejects duplicate keys',()=>assert.throws(()=>validate(tx({ro:[key,key]}),[...keys,...keys])));
test('rejects changed extension duration',()=>assert.throws(()=>validate(tx({extendTo:EXTEND+1}),keys)));
test('rejects excessive fee',()=>assert.throws(()=>validate(tx({fee:'400000001'}),keys)));
test('rejects wrong network',()=>assert.throws(()=>validate(tx({network:S.Networks.TESTNET}),keys)));
test('rejects additional operation',()=>assert.throws(()=>validate(tx({second:true}),keys)));
