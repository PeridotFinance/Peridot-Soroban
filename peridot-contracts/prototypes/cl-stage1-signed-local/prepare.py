"""Isolated additions to the existing runtime29 harness; unchanged financial WASMs."""
from pathlib import Path
import shutil,json
root=Path(__file__).resolve().parents[2];here=Path(__file__).resolve().parent
out=root/'target/cl-stage1-signed-local';h=out/'harness';h.mkdir(exist_ok=False)
base=root/'target/cl-stage1-cash-first-build-v4/harness'
shutil.copytree(base/'src',h/'src')
for n in ['Cargo.toml','Cargo.lock']:shutil.copy2(base/n,h/n)
p=h/'src/lib.rs';s=p.read_text().replace(str(root/'harness/cl-runtime29/src/transaction_evidence.rs'),str(h/'src/transaction_evidence.rs'));s+='\n#[path = "'+str(here/'signed.rs')+'"]\nmod signed_local;\n';p.write_text(s)
s=(root/'harness/cl-runtime29/src/transaction_evidence.rs').read_text()
needle='    let snapshot = Rc::new(snapshot);'
replacement='''    if let Ok(dir) = std::env::var("CL_PRIVATE_CAPTURE_DIR") {
        let test=std::thread::current().name().unwrap_or("unnamed").replace("::","_");
        let path=std::path::Path::new(&dir).join(format!("{test}-{}-{name}.snapshot.json",info.sequence_number));
        std::fs::write(path,serde_json::to_vec(&snapshot).unwrap()).unwrap();
    }
'''+needle
assert s.count(needle)==1;s=s.replace(needle,replacement)
# Isolated public entry for the signed SDK test; reuse the existing host/fee exporter.
s=s.replace('fn simulate_one(', 'pub(crate) fn simulate_one(')
s=s.replace('let source = AccountId(PublicKey::PublicKeyTypeEd25519(Uint256([0; 32])));','''let private_input:Option<Value>=std::env::var("CL_PRIVATE_INPUT").ok().map(|p|serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap());
    let source = if let Some(input)=&private_input {
        AccountId::from_xdr_base64(input["source_account_xdr"].as_str().unwrap(),Limits::none()).unwrap()
    } else {AccountId(PublicKey::PublicKeyTypeEd25519(Uint256([0;32])))};''')
s=s.replace('if let Some(wrong) = wrong_args {','''if let Some(input)=&private_input {
            if input["mode"]=="record" {RecordingInvocationAuthMode::recording(false,false)} else {
                RecordingInvocationAuthMode::Enforcing(input["auth_xdr"].as_array().unwrap().iter().map(|v|SorobanAuthorizationEntry::from_xdr_base64(v.as_str().unwrap(),Limits::none()).unwrap()).collect())
            }
        } else if let Some(wrong) = wrong_args {''',1)
s=s.replace('if address != allowed.as_ref() {','if private_input.is_none() && address != allowed.as_ref() {',1)
# Keep standard unsigned envelope schema for preflight, explicitly annotate actual auth mode below.
s=s.replace('    let allowed = external.map', '    let allowed = external.map')
s=s.replace('"signature_verification_measured":false,','"signature_verification_measured":false,"host_address_signature_verification_measured":private_input.as_ref().map(|v|v["mode"]=="enforcing-detached").unwrap_or(false),')
(h/'src/transaction_evidence.rs').write_text(s)
p=h/'src/test.rs';p.write_text(p.read_text().replace('pub(super) fn measure_scope(','pub(crate) fn measure_scope('))
print(h)
