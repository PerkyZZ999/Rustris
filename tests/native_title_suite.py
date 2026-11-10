#!/usr/bin/python3
"""Explicitly authorized Minecraft native-title smoke. No world is opened."""
from pathlib import Path
import hashlib
from native_support import library,ROOT,BINARY,report,wait,find,visible,press,text,qt,check,db,scalar,nodes,gi,json,os,subprocess,time,sys
GAME=Path(sys.argv[4]).resolve()
from private_paths import private_path
assert GAME==private_path('minecraft')
def inventory():
    return {str(p.relative_to(GAME)):(p.stat().st_size,p.stat().st_mtime_ns) for p in GAME.rglob('*') if p.is_file()}
def output():
    node=find('Captured session output','text')
    return gi.repository.Atspi.Text.get_text(node,0,-1) if node else ''
def state():
    with db() as c:
        row=c.execute('SELECT state FROM launch_sessions ORDER BY created_ms DESC LIMIT 1').fetchone()
        return row[0] if row else None
before=inventory()
critical=['start.sh','java/bin/java','versions/26.3/26.3.jar']
hashes={name:hashlib.sha256((GAME/name).read_bytes()).hexdigest() for name in critical}
report['title']={'name':'Minecraft 26.3 Java Edition','root':str(GAME),'critical_sha256':hashes,'network':'private namespace unavailable','world_opened':False,'authorization':'Charles identified this title and start.sh in this chat on 2026-10-26'}
try:
    wait(lambda:find('Search games','text'))
    script=ROOT/'window-observer.js'
    script.write_text('function observe(w) { print("RUSTRIS_TITLE_WINDOW " + JSON.stringify({caption:w.caption,resourceClass:String(w.resourceClass),pid:w.pid,width:w.frameGeometry.width,height:w.frameGeometry.height})); } workspace.windowAdded.connect(observe); workspace.windowList().forEach(observe);')
    loaded=subprocess.run(['qdbus6','org.kde.KWin','/Scripting','org.kde.kwin.Scripting.loadScript',str(script),'rustris-title-observer'],capture_output=True,text=True,check=True,timeout=10)
    index=int(loaded.stdout.strip())
    subprocess.run(['qdbus6','org.kde.KWin',f'/Scripting/Script{index}','org.kde.kwin.Script.run'],check=True,capture_output=True,timeout=10)
    press('Add game');wait(lambda:find('Game name','text'))
    text('Game name','Minecraft 26.3 (native acceptance)')
    text('Executable path',str(GAME/'start.sh'));text('Working directory',str(GAME))
    press('Save profile');wait(lambda:visible('Play','button'))
    press('Play');wait(lambda:state()=='running',timeout=30)
    check('minecraft-owned-native-launch',scalar('SELECT count(*) FROM session_reservations')==1)
    def game_window():
        lines=(ROOT/'kwin.log').read_text(errors='replace').splitlines()
        for line in lines:
            if 'RUSTRIS_TITLE_WINDOW ' in line:
                value=json.loads(line.split('RUSTRIS_TITLE_WINDOW ',1)[1])
                if 'minecraft' in value['caption'].casefold() or 'minecraft' in value['resourceClass'].casefold():return value
        return None
    window=wait(game_window,timeout=60);report['title']['window']=window
    check('minecraft-native-window-mapped',window['width']>0 and window['height']>0)
    wait(lambda:'LWJGL' in output() or 'OpenGL' in output(),timeout=40)
    check('minecraft-renderer-output-captured',True)
    time.sleep(3)
    check('minecraft-scope-remains-observed',state()=='running')
    qt('screenshot')
    # A regular owned TERM lets Java run shutdown hooks; no force or world interaction.
    press('Stop');wait(lambda:state()=='stopped',timeout=30)
    wait(lambda:scalar('SELECT count(*) FROM session_reservations')==0)
    check('minecraft-owned-graceful-stop-and-release',True)
    (ROOT/'captured-display.txt').write_text(output())
    library();qt('close');wait(lambda:find('Rustris','application') is None)
    check('minecraft-smoke-closes-cleanly',True)
    report['passed']=True
except Exception as error:
    report['passed']=False;report['error']=str(error)
    try:
        if visible('Stop','button'):press('Stop');wait(lambda:state()=='stopped',timeout=15)
    except Exception as cleanup:report['cleanup_error']=str(cleanup)
    raise
finally:
    after=inventory()
    report['title']['changed_files']=[name for name in after if before.get(name)!=after[name]]
    report['title']['removed_files']=[name for name in before if name not in after]
    report['title']['critical_inputs_unchanged']=all(hashlib.sha256((GAME/name).read_bytes()).hexdigest()==value for name,value in hashes.items())
    (ROOT/'report.json').write_text(json.dumps(report,indent=2))
