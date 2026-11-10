"""Compile one test translation unit, relink separate binary; preserve original Core."""
from pathlib import Path
import subprocess,json,hashlib,sys
root=Path(__file__).resolve().parents[2];base=root/'target/cl-option1-core';out=root/'target/cl-stage1-signed-local'/sys.argv[1];out.mkdir(exist_ok=False)
original=Path(__file__).with_name('CLSignedApplication.cpp')
source=out/'CLSignedApplication.cpp';source.write_bytes(original.read_bytes())
(out/'source-sha256.txt').write_text(hashlib.sha256(source.read_bytes()).hexdigest()+'\n')
cmd=json.loads((base/'fixture-final-compile-command.json').read_text())['argv']
cmd=[str(out/'CLSignedApplication.o') if x=='transactions/test/CLRestoreQualification.o' else str(source) if x=='transactions/test/CLRestoreQualification.cpp' else str(out/'compile.d') if x.endswith('FuzzRegressionTests.Tpo') else x for x in cmd]
with (out/'compile.log').open('w')as log:subprocess.run(cmd,cwd=base/'build-core/src',stdout=log,stderr=subprocess.STDOUT,check=True)
link=json.loads((base/'final-link-command.json').read_text())['argv']
link=[str(out/'stellar-core-private')if x=='stellar-core'else str(out/'CLSignedApplication.o')if x=='transactions/test/CLRestoreQualification.o'else x for x in link]
(out/'commands.json').write_text(json.dumps({'compile':cmd,'link':link},indent=2))
with(out/'link.log').open('w')as log:subprocess.run(link,cwd=base/'build-core/src',stdout=log,stderr=subprocess.STDOUT,check=True)
print(hashlib.sha256((out/'stellar-core-private').read_bytes()).hexdigest())
