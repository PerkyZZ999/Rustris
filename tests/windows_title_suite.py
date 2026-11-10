#!/usr/bin/python3
"""User-authorized Dragonwilds smoke in the private offline desktop."""
from pathlib import Path
from native_support import Atspi,ROOT,report,wait,find,visible,press,text,qt,check,db,scalar,nodes,gi,json,os,subprocess,time,sys, runner_paths

from private_paths import private_path
GAME=private_path('dragonwilds_game')
PREFIX=private_path('dragonwilds_prefix')
inputs=Path(sys.argv[4])
def inventory(base):
    result={}
    for parent,dirs,files in os.walk(base,followlinks=False):
        for name in dirs+files:
            path=Path(parent)/name
            try:
                st=path.lstat();result[str(path.relative_to(base))]=(st.st_mode,st.st_size,st.st_mtime_ns)
            except FileNotFoundError:pass
    return result
def state():
    with db() as connection:
        row=connection.execute('SELECT state FROM launch_sessions ORDER BY created_ms DESC LIMIT 1').fetchone()
        return row[0] if row else None
def game_window():
    for line in (ROOT/'kwin.log').read_text(errors='replace').splitlines():
        if 'RUSTRIS_TITLE_WINDOW ' in line:
            value=json.loads(line.split('RUSTRIS_TITLE_WINDOW ',1)[1])
            if any(x in (value['caption']+' '+value['resourceClass']).casefold() for x in ('dragonwilds','rsdragonwilds')):return value
    return None
before={str(path):inventory(path) for path in (GAME,PREFIX)}
report['title']={'name':'RuneScape Dragonwilds','executable':str(GAME/'RSDragonwilds.exe'),'prefix':str(PREFIX),'network':'private namespace unavailable','world_opened':False,'runner_inputs':'private copies','authorization':'User identified game and prefix and accepted normal writes in this chat'}
try:
    wait(lambda:find('Search games','text'))
    script=ROOT/'window-observer.js'
    script.write_text('function observe(w) { print("RUSTRIS_TITLE_WINDOW " + JSON.stringify({caption:w.caption,resourceClass:String(w.resourceClass),pid:w.pid,width:w.frameGeometry.width,height:w.frameGeometry.height})); } workspace.windowAdded.connect(observe); workspace.windowList().forEach(observe);')
    loaded=subprocess.run(['qdbus6','org.kde.KWin','/Scripting','org.kde.kwin.Scripting.loadScript',str(script),'rustris-title-observer'],capture_output=True,text=True,check=True,timeout=10)
    subprocess.run(['qdbus6','org.kde.KWin',f'/Scripting/Script{int(loaded.stdout.strip())}','org.kde.kwin.Script.run'],check=True,capture_output=True,timeout=10)
    press('Settings');press('Runner settings');runner_paths()
    for name,value in [('UMU executable','/usr/bin/umu-run'),('Proton directory',str(inputs/'proton')),('Prepared runtime directory',str(inputs/'data/umu/steamrt3'))]:text(name,value)
    press('Save runner settings');wait(lambda:not visible('Save runner settings','button'))
    press('Add game');text('Game name','Dragonwilds acceptance');text('Executable path',str(GAME/'RSDragonwilds.exe'));text('Working directory',str(GAME))
    press('UMU / Proton','radio button');text('Wine prefix',str(PREFIX));press('Save profile')
    wait(lambda:visible('Review and play','button'));press('Review and play');wait(lambda:visible('Launch game','button'),timeout=30)
    launch=visible('Launch game','button');assert launch.get_state_set().contains(Atspi.StateType.ENABLED),'Preflight did not prepare a runnable plan'
    check('review-ready-before-real-title',True);press('Launch game')
    wait(lambda:state() in ('running','exited','launch_failed','stopped'),timeout=40)
    report['title']['initial_session_state']=state()
    check('real-title-session-created',True)
    deadline=time.monotonic()+50
    while time.monotonic()<deadline and not game_window() and state() in ('running','preparing'):
        time.sleep(.25)
    window=game_window();report['title']['window']=window;report['title']['state_after_observation']=state()
    if window:check('dragonwilds-window-mapped',window['width']>0 and window['height']>0)
    if state() in ('running','stop_requested'):
        if state()=='running':
            wait(lambda:visible('Stop','button'),timeout=10);press('Stop')
        wait(lambda:state() in ('stopped','exited','launch_failed'),timeout=30)
        check('owned-scope-terminal',True)
    report['title']['terminal_state']=state()
    report['passed']=bool(window and state() in ('stopped','exited'))
finally:
    try:
        if state()=='running' and visible('Stop','button'):
            press('Stop');wait(lambda:state() in ('stopped','exited','launch_failed'),timeout=30)
    except Exception as error:report['cleanup_error']=str(error)
    after={str(path):inventory(path) for path in (GAME,PREFIX)}
    report['title']['metadata_changes']={str(path):{'changed_or_added':sum(before[str(path)].get(k)!=v for k,v in after[str(path)].items()),'removed':sum(k not in after[str(path)] for k in before[str(path)])} for path in (GAME,PREFIX)}
    (ROOT/'report.json').write_text(json.dumps(report,indent=2))

if not report.get('passed',False):
    raise SystemExit(1)
