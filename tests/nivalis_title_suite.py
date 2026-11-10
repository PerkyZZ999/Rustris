#!/usr/bin/python3
"""User-identified Nivalis smoke with a fresh prefix; original game/prefix are read-only."""
from pathlib import Path
import hashlib
from umu_fixture_support import identified_runner_copy
from native_support import ROOT,report,wait,find,visible,press,text,qt,check,db,scalar,nodes,Atspi,json,os,subprocess,time,sys, runner_paths

from private_paths import private_path
GAME=private_path('nivalis_game')
EXE=GAME/'Nivalis Nights.exe'
ORIGINAL_PREFIX=private_path('nivalis_prefix')
INPUTS=Path(sys.argv[4]).resolve()
GE_PREFLIGHT='--ge-preflight' in sys.argv
PREFLIGHT='--preflight' in sys.argv or GE_PREFLIGHT
PREFIX=ORIGINAL_PREFIX if PREFLIGHT else ROOT/'disposable-prefix'
owned_runtime=(Path(__file__).resolve().parents[1]/'artifacts/s3/runtime').resolve()
if not identified_runner_copy(INPUTS,owned_runtime):
    raise SystemExit('Only identified private runner copies are supported')
RUNTIME='steamrt4' if (INPUTS/'data/umu/steamrt4').is_dir() else 'steamrt3'
if not PREFLIGHT:PREFIX.mkdir(mode=0o700)

def fingerprint(base):
    records=[]
    for parent,dirs,files in os.walk(base,followlinks=False):
        for name in dirs+files:
            path=Path(parent)/name
            try:
                st=path.lstat();records.append((os.fsencode(path.relative_to(base)).hex(),st.st_mode,st.st_size,st.st_mtime_ns,os.readlink(path) if path.is_symlink() else None))
            except FileNotFoundError:pass
    return hashlib.sha256(json.dumps(sorted(records)).encode()).hexdigest(),len(records)
def state():
    with db() as connection:
        row=connection.execute('SELECT state FROM launch_sessions ORDER BY created_ms DESC LIMIT 1').fetchone()
        return row[0] if row else None
def window():
    for line in (ROOT/'kwin.log').read_text(errors='replace').splitlines():
        if 'RUSTRIS_TITLE_WINDOW ' in line:
            value=json.loads(line.split('RUSTRIS_TITLE_WINDOW ',1)[1])
            if 'nivalis' in (value['caption']+' '+value['resourceClass']).casefold():return value
    return None
before={str(path):fingerprint(path) for path in (GAME,ORIGINAL_PREFIX)}
exe_hash=hashlib.sha256(EXE.read_bytes()).hexdigest()
version_before=(ORIGINAL_PREFIX/'version').read_text().strip()
assert version_before=='GE-Proton11-5'
report['title']={'name':'Nivalis Nights','executable':str(EXE),'original_prefix':str(ORIGINAL_PREFIX),'original_prefix_version':version_before,'used_prefix':'read-only original for preflight' if PREFLIGHT else 'fresh disposable private prefix','runner_inputs':f'private {RUNTIME} runner copies','game_mount':'read-only','network':'unavailable in private namespace','audio':'host audio unavailable','world_opened':False,'input_sha256':exe_hash,'authorization':'User identified game and prefix in this chat; the original prefix was never launched'}
try:
    wait(lambda:find('Search games','text'))
    script=ROOT/'window-observer.js'
    script.write_text('function observe(w) { print("RUSTRIS_TITLE_WINDOW " + JSON.stringify({caption:w.caption,resourceClass:String(w.resourceClass),pid:w.pid,width:w.frameGeometry.width,height:w.frameGeometry.height})); } workspace.windowAdded.connect(observe); workspace.windowList().forEach(observe);')
    loaded=subprocess.run(['qdbus6','org.kde.KWin','/Scripting','org.kde.kwin.Scripting.loadScript',str(script),'rustris-nivalis-observer'],capture_output=True,text=True,check=True,timeout=10)
    subprocess.run(['qdbus6','org.kde.KWin',f'/Scripting/Script{int(loaded.stdout.strip())}','org.kde.kwin.Script.run'],check=True,capture_output=True,timeout=10)
    press('Settings');press('Runner settings');runner_paths()
    for name,value in [('UMU executable','/usr/bin/umu-run'),('Proton directory',str(INPUTS/'proton')),('Prepared runtime directory',str(INPUTS/'data/umu'/RUNTIME))]:text(name,value)
    press('Save runner settings');wait(lambda:not visible('Save runner settings','button'))
    press('Add game');text('Game name','Nivalis Nights acceptance');text('Executable path',str(EXE));text('Working directory',str(GAME))
    press('UMU / Proton','radio button');text('Wine prefix',str(PREFIX));press('Save profile')
    wait(lambda:visible('Review and play','button'));press('Review and play');wait(lambda:visible('Launch game','button'),timeout=30)
    launch=visible('Launch game','button')
    if PREFLIGHT:
        if GE_PREFLIGHT:
            assert RUNTIME=='steamrt4' and launch.get_state_set().contains(Atspi.StateType.ENABLED)
            assert any('GE-Proton11-5' in node.get_name() and '4.0.20261021.265997' in node.get_name() for node in nodes())
            assert state() is None
            check('matching-existing-prefix-preflight-only',True)
            report['passed']=True
            raise SystemExit(0)
        assert not launch.get_state_set().contains(Atspi.StateType.ENABLED)
        assert any('umu.prefix_version.preparation' in node.get_name() for node in nodes())
        assert state() is None
        check('existing-prefix-version-mismatch-blocks-before-session',True)
        report['passed']=True
        raise SystemExit(0)
    assert launch.get_state_set().contains(Atspi.StateType.ENABLED),'Preflight did not prepare the isolated prefix'
    check('review-ready-before-launch',not (PREFIX/'drive_c').exists());press('Launch game')
    wait(lambda:state() in ('running','exited','launch_failed','stopped'),timeout=40)
    report['title']['initial_session_state']=state();check('session-created',True)
    deadline=time.monotonic()+50
    while time.monotonic()<deadline and not window() and state() in ('running','preparing'):
        time.sleep(.25)
    found=window();report['title']['window']=found;report['title']['state_after_observation']=state()
    if found:check('nivalis-window-mapped',found['width']>0 and found['height']>0)
    if state()=='running':
        wait(lambda:visible('Stop','button'),timeout=10);press('Stop')
        wait(lambda:state() in ('stopped','exited','launch_failed'),timeout=30)
        check('owned-scope-terminal',True)
    report['title']['terminal_state']=state()
    report['passed']=bool(found and state() in ('stopped','exited'))
finally:
    try:
        if state()=='running' and visible('Stop','button'):
            press('Stop');wait(lambda:state() in ('stopped','exited','launch_failed'),timeout=30)
    except Exception as error:report['cleanup_error']=str(error)
    after={str(path):fingerprint(path) for path in (GAME,ORIGINAL_PREFIX)}
    report['title']['metadata_before_after_equal']={str(path):before[str(path)]==after[str(path)] for path in (GAME,ORIGINAL_PREFIX)}
    report['title']['exe_hash_unchanged']=hashlib.sha256(EXE.read_bytes()).hexdigest()==exe_hash
    report['title']['original_prefix_marker_unchanged']=(ORIGINAL_PREFIX/'version').read_text().strip()==version_before
    (ROOT/'report.json').write_text(json.dumps(report,indent=2))
if not report.get('passed',False):raise SystemExit(1)
