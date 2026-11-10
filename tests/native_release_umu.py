#!/usr/bin/python3
"""Copied default desktop and helper run an isolated prepared Windows fixture."""
import json, os, sqlite3, subprocess, sys, time
from pathlib import Path
from umu_fixture_support import identified_runner_copy
root=Path(sys.argv[1]);inputs=Path(sys.argv[2]).resolve();database=root/'data/rustris/library.sqlite3';prefix=root/'prefix'
owned_runtime=(Path(__file__).resolve().parents[1]/'artifacts/s3/runtime').resolve()
if not identified_runner_copy(inputs,owned_runtime):
    raise SystemExit('Only an identified private UMU fixture tree is accepted')
runtime='steamrt4' if (inputs/'data/umu/steamrt4').is_dir() else 'steamrt3'
if len(sys.argv)>3 and sys.argv[3]=='--prepare':
    subprocess.run([sys.executable,str(Path(__file__).with_name('native_release_artwork.py')),str(root),'--prepare'],check=True)
    prefix.mkdir(mode=0o700)
    config=root/'config/rustris';config.mkdir(mode=0o700)
    document='version = 1\n'+''.join(f'{name} = {json.dumps(str(value))}\n' for name,value in [('umu','/usr/bin/umu-run'),('proton',inputs/'proton'),('runtime',inputs/'data/umu'/runtime)])
    (config/'runners.toml').write_text(document);(config/'runners.toml').chmod(0o600)
    with sqlite3.connect(database) as connection:
        profile=connection.execute('SELECT id FROM profiles').fetchone()[0]
        connection.execute("UPDATE profiles SET executable=?,cwd=?,runner='umu',prefix=? WHERE id=?",(os.fsencode(inputs/'windows-fixture.exe'),os.fsencode(root),os.fsencode(prefix),profile))
        args=['Z:'+str(root/'windows.json').replace('/','\\'),'hold','8000','spaces and 🦀','$(touch never)']
        connection.executemany('INSERT INTO arguments VALUES(?,?,?)',[(profile,i,value.encode()) for i,value in enumerate(args)])
    raise SystemExit(0)
import gi
gi.require_version('Atspi','2.0')
from gi.repository import Atspi
def walk(node):
    if node is None:return
    try:
        yield node
        for i in range(node.get_child_count()):yield from walk(node.get_child_at_index(i))
    except gi.repository.GLib.Error:return
def find(name,role=None):
    return next((n for n in walk(Atspi.get_desktop(0)) if n.get_name()==name and (role is None or n.get_role_name()==role)),None)
def wait(predicate,timeout=30):
    deadline=time.monotonic()+timeout
    while time.monotonic()<deadline:
        try:value=predicate()
        except gi.repository.GLib.Error:value=None
        if value:return value
        time.sleep(.03)
    raise AssertionError('Expected copied release UMU state not observed')
def press(name):
    node=wait(lambda:find(name,'button'))
    wait(lambda:node.get_state_set().contains(Atspi.StateType.ENABLED))
    action=node.get_action_iface();assert action
    for i in range(action.get_n_actions()):
        if action.get_action_name(i).casefold() in ('press','click'):
            assert action.do_action(i);return
    raise AssertionError('Native action unavailable: '+name)
def state():
    with sqlite3.connect(database) as connection:
        row=connection.execute('SELECT state FROM launch_sessions ORDER BY created_ms DESC LIMIT 1').fetchone()
        return row[0] if row else None
report={'passed':False,'checks':[]}
try:
    press('Review and play');wait(lambda:find('Launch game','button'),timeout=30)
    assert state() is None and not (prefix/'drive_c').exists();report['checks'].append('copied-release-review-does-not-spawn')
    wait(lambda:any(n.get_name()=='Cancel' and n.get_state_set().contains(Atspi.StateType.FOCUSED) for n in walk(Atspi.get_desktop(0))))
    report['checks'].append('copied-release-review-focuses-cancel')
    press('Launch game')
    wait(lambda:any(n.get_name()=='Choose session' and n.get_state_set().contains(Atspi.StateType.FOCUSED) for n in walk(Atspi.get_desktop(0))))
    report['checks'].append('copied-release-launch-focuses-session')
    wait(lambda:(root/'windows.json').exists(),timeout=60)
    result=json.loads((root/'windows.json').read_text())
    assert result['argv'][4:]==['spaces and 🦀','$(touch never)'] and not (root/'never').exists()
    report['checks'].append('copied-release-runs-private-windows-fixture')
    wait(lambda:find('Stop','button'));press('Stop');wait(lambda:state()=='stopped',timeout=30)
    with sqlite3.connect(database) as connection:assert connection.execute('SELECT count(*) FROM session_reservations').fetchone()[0]==0
    report['checks'].append('copied-release-owned-stop')
    assert not (root/'qt-response.json').exists();report['checks'].append('copied-release-has-no-test-transport')
    report['passed']=True
finally:(root/'release-umu-report.json').write_text(json.dumps(report,indent=2))
