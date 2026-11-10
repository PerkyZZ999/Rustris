#!/usr/bin/env python3
"""Prepare only identified runtime copies, then run explicit production UMU fixtures."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
from umu_fixture_support import run_fixture

parser=argparse.ArgumentParser()
parser.add_argument('--root',required=True,type=Path)
parser.add_argument('--test-binary',required=True,type=Path)
args=parser.parse_args()
root=args.root.resolve();root.mkdir(parents=True,mode=0o700,exist_ok=False)
from private_paths import private_path
proton=private_path('umu_proton')
runtime=private_path('umu_runtime')
repo=Path(__file__).resolve().parents[1]

def fingerprint(base):
    records=[]
    for path in [base,*sorted(base.rglob('*'))]:
        stat=path.lstat()
        records.append((str(path.relative_to(base)),stat.st_mode,stat.st_size,stat.st_mtime_ns,os.readlink(path) if path.is_symlink() else None))
    return hashlib.sha256(json.dumps(records).encode()).hexdigest(),len(records)

before={str(path):fingerprint(path) for path in (proton,runtime)}
if (proton/'user_settings.py').exists():raise SystemExit('Custom runner settings require review')
assert (proton/'version').read_text().strip().endswith('UMU-Proton-10.0-4')
assert '3.0.20260805.254768' in (runtime/'VERSIONS.txt').read_text()
(root/'data/umu').mkdir(parents=True,mode=0o700)
for source,target in [(proton,root/'proton'),(runtime,root/'data/umu/steamrt3')]:
    subprocess.run(['cp','-a','--reflink=auto',str(source),str(target)],check=True)
subprocess.run(['cp','-a',str(runtime.parent/'umu-shim'),str(root/'data/umu/umu-shim')],check=True)
subprocess.run(['x86_64-w64-mingw32-gcc','-O2','-Wall','-Wextra','-Werror','-municode','-static',str(repo/'experiments/f003/src/fixture.c'),'-o',str(root/'windows-fixture.exe')],check=True)
for name in ('home','cache','config','runtime','tmp','cases'):(root/name).mkdir(mode=0o700)
env=os.environ.copy()
for key in ('DISPLAY','WAYLAND_DISPLAY','DBUS_SESSION_BUS_ADDRESS','PYTHONPATH','PYTHONHOME'):
    env.pop(key,None)
env.update(F003_TOKEN='synthetic-never-inherit',RUSTRIS_UMU_FIXTURE_ROOT=str(root),HOME=str(root/'home'),XDG_DATA_HOME=str(root/'data'),XDG_CONFIG_HOME=str(root/'config'),XDG_CACHE_HOME=str(root/'cache'),XDG_RUNTIME_DIR=str(root/'runtime'),TMPDIR=str(root/'tmp'),LANG='C.UTF-8',PATH='/usr/bin:/bin')
command=['bwrap','--die-with-parent','--unshare-net','--unshare-pid','--ro-bind','/','/','--dev','/dev','--proc','/proc','--tmpfs','/tmp','--bind',str(root),str(root),'--',str(args.test_binary.resolve()),'actual_umu_','--ignored','--test-threads=1','--nocapture']
test_exit=run_fixture(command,env,timeout=180)
after={str(path):fingerprint(path) for path in (proton,runtime)}
report={'source_before':before,'source_after':after,'source_trees_unchanged':before==after,'network':'private network namespace configured for fixture command','root':str(root),'test_exit':test_exit,'timed_out':test_exit is None,'windows_fixture_source':'experiments/f003/src/fixture.c; only the harmless test executable is reused, never its prototype helper'}
(root/'fixture-report.json').write_text(json.dumps(report,indent=2))
assert before==after,'Source runner/runtime metadata changed'
raise SystemExit(124 if test_exit is None else test_exit)
