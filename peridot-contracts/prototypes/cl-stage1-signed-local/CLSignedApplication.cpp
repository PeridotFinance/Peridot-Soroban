// Isolated private-ledger application; signatures produced only by public-test-key fixture.
#include "bucket/test/BucketTestUtils.h"
#include "crypto/Hex.h"
#include "crypto/SHA.h"
#include "crypto/SecretKey.h"
#include "herder/LedgerCloseData.h"
#include "herder/TxSetFrame.h"
#include "ledger/ImmutableLedgerView.h"
#include "ledger/LedgerManagerImpl.h"
#include "ledger/LedgerTxn.h"
#include "ledger/LedgerTypeUtils.h"
#include "ledger/NetworkConfig.h"
#include "overlay/OverlayManager.h"
#include "test/Catch2.h"
#include "test/TestUtils.h"
#include "test/test.h"
#include "transactions/EventManager.h"
#include "transactions/MutableTransactionResult.h"
#include "transactions/TransactionFrameBase.h"
#include "transactions/TransactionMeta.h"
#include "transactions/TransactionUtils.h"
#include "util/Decoder.h"
#include "util/Logging.h"
#include "util/XDRCereal.h"
#include <chrono>
#include <cstdlib>
#include <fstream>
#include <iostream>
#include <limits>
#include <set>
#include <xdrpp/marshal.h>
using namespace stellar;
namespace {
constexpr auto PRIVATE_NETWORK="Peridot Stage1 signature application tests ONLY 2026-11-10";
class PrivateOfflineApplication:public TestApplication {
 public:PrivateOfflineApplication(VirtualClock& c,Config const& f):TestApplication(c,f){}
 OverlayManager& getOverlayManager()override{throw std::runtime_error("private test has no overlay");}
 private:std::unique_ptr<OverlayManager> createOverlayManager()override{return nullptr;}
};
template<typename T>T readXdr(std::istream& in){std::string b;REQUIRE(bool(std::getline(in,b)));std::vector<uint8_t> raw;decoder::decode_b64(b,raw);T v;xdr::xdr_from_opaque(raw,v);return v;}
uint64_t number(std::istream& in){std::string v;REQUIRE(bool(std::getline(in,v)));return std::stoull(v);}
}
TEST_CASE("private signed Stage1 Core29 application","[cl-stage1-private-signed]") {
 REQUIRE(std::getenv("CL_CORE_FORBID_SIGNING")!=nullptr); // Core itself may never sign.
 auto inputPath=std::getenv("CL_STAGE1_PRIVATE_CORE_INPUT");REQUIRE(inputPath!=nullptr);
 std::ifstream in(inputPath);REQUIRE(in.good());std::string network;REQUIRE(bool(std::getline(in,network)));REQUIRE(network==PRIVATE_NETWORK);
 auto ledger=static_cast<uint32_t>(number(in));auto timestamp=number(in);
 auto envelope=readXdr<TransactionEnvelope>(in);REQUIRE(envelope.type()==ENVELOPE_TYPE_TX);
 REQUIRE(envelope.v1().signatures.size()==1);REQUIRE(envelope.v1().tx.operations.size()==1);
 REQUIRE(!envelope.v1().tx.operations[0].sourceAccount);
 auto kind=envelope.v1().tx.operations[0].body.type();REQUIRE((kind==INVOKE_HOST_FUNCTION||kind==RESTORE_FOOTPRINT));
 auto configCount=number(in);REQUIRE(configCount==13);std::vector<LedgerEntry> settings;
 for(size_t i=0;i<configCount;++i){LedgerEntry x;x.lastModifiedLedgerSeq=ledger-1;x.data=readXdr<decltype(x.data)>(in);REQUIRE(x.data.type()==CONFIG_SETTING);settings.push_back(x);}
 auto coldPath=std::getenv("CL_STAGE1_PRIVATE_COLD_DIR");
 std::vector<TransactionEnvelope> preparation;std::set<LedgerKey> coldKeys;
 if(coldPath){std::ifstream chain(std::string(coldPath)+"/chain.txt");REQUIRE(chain.good());auto n=number(chain);REQUIRE(n>0);REQUIRE(n<20);
  for(size_t i=0;i<n;++i){auto tx=readXdr<TransactionEnvelope>(chain);REQUIRE(tx.type()==ENVELOPE_TYPE_TX);REQUIRE(tx.v1().signatures.size()==1);REQUIRE(tx.v1().tx.operations.size()==1);REQUIRE(tx.v1().tx.operations[0].body.type()==RESTORE_FOOTPRINT);REQUIRE(tx.v1().tx.ext.v()==1);
   for(auto const& k:tx.v1().tx.ext.sorobanData().resources.footprint.readWrite){REQUIRE(isPersistentEntry(k));coldKeys.insert(k);}preparation.push_back(tx);}
 }
 auto count=number(in);std::vector<LedgerEntry> initial,original,archived;int64_t syntheticCoins=0;
 for(size_t i=0;i<count;++i){auto e=readXdr<LedgerEntry>(in);auto ttl=static_cast<uint32_t>(number(in));e.lastModifiedLedgerSeq=ledger-1;
  if(e.data.type()==ACCOUNT){REQUIRE(e.data.account().balance>=0);REQUIRE(syntheticCoins<std::numeric_limits<int64_t>::max()-e.data.account().balance);syntheticCoins+=e.data.account().balance;}
  original.push_back(e);if(coldKeys.count(LedgerEntryKey(e))){archived.push_back(e);continue;}
  initial.push_back(e);if(isSorobanEntry(e.data)){auto t=getTTLEntryForTTLKey(getTTLKey(e),ttl);t.lastModifiedLedgerSeq=ledger-1;initial.push_back(t);}
 }
 auto cfg=getTestConfig(0,Config::TESTDB_BUCKET_DB_VOLATILE);cfg.TESTING_UPGRADE_LEDGER_PROTOCOL_VERSION=29;cfg.LEDGER_PROTOCOL_VERSION=29;cfg.USE_CONFIG_FOR_GENESIS=true;
 cfg.RUN_STANDALONE=true;cfg.FORCE_SCP=false;cfg.NODE_IS_VALIDATOR=false;cfg.PARALLEL_LEDGER_APPLY=false;cfg.PEER_PORT=0;cfg.HTTP_PORT=0;cfg.NTP_DRIFT_CHECK_SERVER="";
 cfg.NETWORK_PASSPHRASE=PRIVATE_NETWORK;cfg.KNOWN_PEERS.clear();cfg.PREFERRED_PEERS.clear();cfg.ENABLE_SOROBAN_DIAGNOSTIC_EVENTS=true;
 VirtualClock clock;REQUIRE(!Logging::getLedgerLogPtr()->should_log(spdlog::level::info));
 auto app=createTestApplication<PrivateOfflineApplication>(clock,cfg,true,false);auto& lm=app->getLedgerManager();lm.loadLastKnownLedger();
 auto header=lm.getLastClosedLedgerHeader().header;header.ledgerSeq=ledger-1;header.ledgerVersion=29;header.scpValue.closeTime=timestamp-1;
 REQUIRE(header.totalCoins<std::numeric_limits<int64_t>::max()-syntheticCoins);header.totalCoins+=syntheticCoins;
 {LedgerTxn ltx(app->getLedgerTxnRoot());ltx.loadHeader().current()=header;ltx.commit();}
 BucketTestUtils::addLiveBatchAndUpdateSnapshot(*app,header,initial,settings,{});
 if(coldPath){REQUIRE(archived.size()==coldKeys.size());BucketTestUtils::addHotArchiveBatchAndUpdateSnapshot(*app,header,archived,{});}
 // Imported genesis must use the supported catchup rebuild, including compiled
 // module cache. A memory-state-only rebuild leaves an artificial empty cache.
 auto importedHeader=lm.getLastClosedLedgerHeader();
 lm.markApplyStateReset();lm.setLastClosedLedger(importedHeader,true);
 auto const& nc=lm.getLastClosedSorobanNetworkConfig();REQUIRE(nc.txMaxInstructions()==100'000'000);REQUIRE(nc.txMaxFootprintEntries()==100);REQUIRE(nc.txMaxWriteLedgerEntries()==50);REQUIRE(nc.txMemoryLimit()==41'943'040);
 if(coldPath){
  for(size_t i=0;i<preparation.size();++i){
   auto before=lm.getLastClosedLedgerHeader().header;auto f=TransactionFrameBase::makeTransactionFromWire(app->getNetworkID(),preparation[i]);int64_t balance;
   {LedgerTxn ltx(app->getLedgerTxnRoot());auto a=loadAccount(ltx,f->getSourceID());REQUIRE(a);balance=a.current().data.account().balance;REQUIRE(a.current().data.account().seqNum==f->getSeqNum()-1);
    for(auto const& k:preparation[i].v1().tx.ext.sorobanData().resources.footprint.readWrite)REQUIRE(!ltx.loadWithoutRecord(k));}
   auto diag=DiagnosticEventManager::createDisabled();PubKeyUtils::clearVerifySigCache();auto start=std::chrono::steady_clock::now();
   REQUIRE(f->checkValid(app->getAppConnector(),CheckValidLedgerViewWrapper(*app),0,1,1,diag)->isSuccess());
   PerPhaseTransactionList phases(2);phases[1].push_back(f);auto set=makeTxSetFromTransactions(phases,*app,1,1);REQUIRE(set.second->checkValid(*app,1,1));
   StellarValue value;value.txSetHash=set.first->getContentsHash();value.closeTime=before.scpValue.closeTime+1;value.ext.v(STELLAR_VALUE_BASIC);
   lm.applyLedger(LedgerCloseData(before.ledgerSeq+1,set.first,value),false);
   auto elapsed=std::chrono::duration_cast<std::chrono::nanoseconds>(std::chrono::steady_clock::now()-start).count();
   auto const& r=static_cast<LedgerManagerImpl&>(lm).mLatestTxResultSet.results.at(0).result;
   std::cout<<"CL_PRIVATE_RESTORE_RESULT "<<xdrToCerealString(r,"result")<<std::endl;
   std::cout<<"CL_PRIVATE_RESTORE_META "<<xdrToCerealString(lm.getLastClosedLedgerTxMeta().at(0).getXDR(),"meta")<<std::endl;
   REQUIRE(r.result.code()==txSUCCESS);REQUIRE(r.result.results()[0].tr().restoreFootprintResult().code()==RESTORE_FOOTPRINT_SUCCESS);
   auto ttlExpected=before.ledgerSeq+nc.stateArchivalSettings().minPersistentTTL;
   {LedgerTxn ltx(app->getLedgerTxnRoot());auto a=loadAccount(ltx,f->getSourceID());REQUIRE(a.current().data.account().seqNum==f->getSeqNum());REQUIRE(balance-a.current().data.account().balance==r.feeCharged);
    for(auto const& k:preparation[i].v1().tx.ext.sorobanData().resources.footprint.readWrite){auto e=ltx.loadWithoutRecord(k);REQUIRE(e);auto t=ltx.loadWithoutRecord(getTTLKey(k));REQUIRE(t);REQUIRE(t.current().data.ttl().liveUntilLedgerSeq==ttlExpected);
     auto expected=std::find_if(archived.begin(),archived.end(),[&](auto const& x){return LedgerEntryKey(x)==k;});REQUIRE(expected!=archived.end());REQUIRE(e.current().data==expected->data);}}
   REQUIRE(lm.getLastClosedLedgerHeader().header.feePool==before.feePool+r.feeCharged);REQUIRE(lm.getLastClosedLedgerHeader().header.totalCoins==before.totalCoins);
   REQUIRE(f->checkValid(app->getAppConnector(),CheckValidLedgerViewWrapper(*app),0,1,1,diag)->getXDR().result.code()==txBAD_SEQ);
   std::cout<<"CL_PRIVATE_RESTORE_SUMMARY {\"batch\":"<<i<<",\"ledger\":"<<before.ledgerSeq+1<<",\"fee_charged\":"<<r.feeCharged<<",\"keys\":"<<preparation[i].v1().tx.ext.sorobanData().resources.footprint.readWrite.size()<<",\"bytes\":"<<preparation[i].v1().tx.ext.sorobanData().resources.diskReadBytes<<",\"live_until\":"<<ttlExpected<<",\"sequence\":"<<f->getSeqNum()<<",\"application_ns\":"<<elapsed<<"}"<<std::endl;
  }
  auto next=lm.getLastClosedLedgerHeader().header;ledger=next.ledgerSeq+1;timestamp=next.scpValue.closeTime+1;header=next;
  {std::ofstream exportState(std::string(coldPath)+"/restored-state.txt");exportState<<ledger<<"\n"<<timestamp<<"\n"<<original.size()<<"\n";LedgerTxn ltx(app->getLedgerTxnRoot());
   for(auto const& old:original){auto k=LedgerEntryKey(old);auto e=ltx.loadWithoutRecord(k);REQUIRE(e);uint32_t ttl=0;if(isSorobanEntry(e.current().data)){auto t=ltx.loadWithoutRecord(getTTLKey(k));REQUIRE(t);ttl=t.current().data.ttl().liveUntilLedgerSeq;}
    exportState<<decoder::encode_b64(xdr::xdr_to_opaque(k))<<"\n"<<decoder::encode_b64(xdr::xdr_to_opaque(e.current()))<<"\n"<<ttl<<"\n";}}
  // Fixed repository helper, private public-fixture keys only; Core remains alive.
  std::string dir(coldPath);REQUIRE(dir.find_first_not_of("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789/_-.")==std::string::npos);
  auto command="python3 prototypes/cl-stage1-signed-local/resimulate-cold.py "+dir;REQUIRE(std::system(command.c_str())==0);
  std::ifstream signedFile(dir+"/signed-private-envelope.xdr");envelope=readXdr<TransactionEnvelope>(signedFile);
  REQUIRE(envelope.v1().signatures.size()==1);REQUIRE(envelope.v1().tx.operations.size()==1);REQUIRE(envelope.v1().tx.operations[0].body.type()==INVOKE_HOST_FUNCTION);
 }
 auto frame=TransactionFrameBase::makeTransactionFromWire(app->getNetworkID(),envelope);int64_t beforeBalance;
 {LedgerTxn ltx(app->getLedgerTxnRoot());auto a=loadAccount(ltx,frame->getSourceID());REQUIRE(a);beforeBalance=a.current().data.account().balance;REQUIRE(a.current().data.account().seqNum==frame->getSeqNum()-1);}
 auto diagnostics=DiagnosticEventManager::createDisabled();
 PubKeyUtils::clearVerifySigCache();
 auto validationStart=std::chrono::steady_clock::now();auto valid=frame->checkValid(app->getAppConnector(),CheckValidLedgerViewWrapper(*app),0,1,1,diagnostics);
 auto validationNanos=std::chrono::duration_cast<std::chrono::nanoseconds>(std::chrono::steady_clock::now()-validationStart).count();
 std::cout<<"CL_PRIVATE_VALIDATION "<<xdrToCerealString(valid->getXDR(),"result")<<std::endl;REQUIRE(valid->isSuccess());
 PerPhaseTransactionList phases(2);phases[1].push_back(frame);auto set=makeTxSetFromTransactions(phases,*app,1,1);REQUIRE(set.second->checkValid(*app,1,1));
 StellarValue value;value.txSetHash=set.first->getContentsHash();value.closeTime=timestamp;value.ext.v(STELLAR_VALUE_BASIC);
 auto started=std::chrono::steady_clock::now();lm.applyLedger(LedgerCloseData(ledger,set.first,value),false);
 auto nanos=std::chrono::duration_cast<std::chrono::nanoseconds>(std::chrono::steady_clock::now()-started).count();
 auto const& results=static_cast<LedgerManagerImpl&>(lm).mLatestTxResultSet;REQUIRE(results.results.size()==1);auto const& result=results.results[0].result;
 std::cout<<"CL_PRIVATE_RESULT "<<xdrToCerealString(result,"result")<<std::endl;
 REQUIRE(lm.getLastClosedLedgerTxMeta().size()==1);auto const& meta=lm.getLastClosedLedgerTxMeta()[0];
 std::cout<<"CL_PRIVATE_META "<<xdrToCerealString(meta.getXDR(),"meta")<<std::endl;
 for(auto const& event:meta.getDiagnosticEvents())std::cout<<"CL_PRIVATE_DIAGNOSTIC "<<xdrToCerealString(event,"event")<<std::endl;
 REQUIRE(result.result.code()==txSUCCESS);int64_t afterBalance;
 {LedgerTxn ltx(app->getLedgerTxnRoot());auto a=loadAccount(ltx,frame->getSourceID());REQUIRE(a.current().data.account().seqNum==frame->getSeqNum());afterBalance=a.current().data.account().balance;}
 REQUIRE(beforeBalance-afterBalance==result.feeCharged);REQUIRE(lm.getLastClosedLedgerHeader().header.feePool==header.feePool+result.feeCharged);REQUIRE(lm.getLastClosedLedgerHeader().header.totalCoins==header.totalCoins);
 auto replay=frame->checkValid(app->getAppConnector(),CheckValidLedgerViewWrapper(*app),0,1,1,diagnostics);REQUIRE(replay->getXDR().result.code()==txBAD_SEQ);
 std::cout<<"CL_PRIVATE_APPLICATION_SUMMARY {\"ledger\":"<<ledger<<",\"fee_charged\":"<<result.feeCharged<<",\"source_debit\":"<<beforeBalance-afterBalance<<",\"sequence\":"<<frame->getSeqNum()<<",\"envelope_signatures\":1,\"validation_ns\":"<<validationNanos<<",\"application_ns\":"<<nanos<<",\"replay_rejected\":true}"<<std::endl;
}
