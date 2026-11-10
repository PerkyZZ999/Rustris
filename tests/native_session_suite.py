#!/usr/bin/python3
"""Production native Play/control/capture/lifetime regression in a private desktop."""
from native_support import library,open_game,ROOT,BINARY,SCALE,Atspi,children,logs,report,wait,find,visible,press,text,qt,check,db,scalar,nodes,gi,json,os,subprocess,time,focused_name

def enabled(name):
    n=visible(name,'button') or visible(name,'push button')
    return n is not None and n.get_state_set().contains(Atspi.StateType.ENABLED)
def state():
    with db() as connection:
        row=connection.execute('SELECT state FROM launch_sessions ORDER BY created_ms DESC LIMIT 1').fetchone()
        return row[0] if row else None
def add(name,executable,arguments=()):
    press('Add game');wait(lambda:find('Game name','text'))
    text('Game name',name);text('Executable path',str(executable));text('Working directory',str(ROOT))
    for i,arg in enumerate(arguments):press('Add argument');text(f'Argument {i+1}',arg)
    press('Save profile');wait(lambda:enabled('Play'))
def output():
    n=find('Captured session output','text')
    return Atspi.Text.get_text(n,0,-1) if n else ''
def close_panel():
    # Logs live in the game's Logs tab or the Sessions page; leave for the library.
    library();wait(lambda:not visible('Captured session output','text'))
def launch_app():
    (ROOT/'qt-request.json').unlink(missing_ok=True)
    log=(ROOT/f'session-restart-{len(logs)}.log').open('w');logs.append(log)
    p=subprocess.Popen([str(BINARY)],stdout=log,stderr=subprocess.STDOUT,cwd=ROOT/'home');children.append(p)
    wait(lambda:find('Search games','text'));return p

try:
    wait(lambda:find('Search games','text'))
    check('session-window-dpr',abs(qt('state')['dpr']-SCALE)<.01)
    add('Missing native fixture',ROOT/'absent')
    press('Play');wait(lambda:state()=='launch_failed')
    press('Preflight evidence')
    wait(lambda:any('native.executable.availability' in n.get_name() for n in nodes()))
    check('preflight-dialog-focuses-back',wait(lambda:focused_name('Back to session')))
    check('preflight-blocks-missing-file-with-evidence',scalar('SELECT helper_pid FROM launch_sessions') is None and scalar('SELECT count(*) FROM session_reservations')==0)
    check('preflight-source-and-time-visible',any('filesystem metadata and access(2)' in n.get_name() and 'Evidence:' in n.get_name() for n in nodes()))
    press('Back to session')
    close_panel()
    fixture=ROOT/'native-control.py'
    fixture.write_text("import os,time,signal,sys\nprint('<b>literal</b> session stdout',flush=True)\nprint('session stderr',file=sys.stderr,flush=True)\nopen('game-pid','w').write(str(os.getpid()))\nsignal.signal(signal.SIGTERM,signal.SIG_IGN)\nfor i in range(600):\n print('heartbeat '+str(i),flush=True);time.sleep(.1)\n")
    add('Owned native fixture','/usr/bin/python3',[str(fixture),'','literal ;$(never)'])
    for _ in range(40):
        if focused_name('Play'):break
        qt('key',sequence='Tab')
    check('keyboard-can-focus-play',focused_name('Play'))
    qt('key',sequence='Space');wait(lambda:state()=='running')
    wait(lambda:enabled('Stop'))
    check('native-play-observed-and-logs-visible',wait(lambda:'<b>literal</b> session stdout' in output() and 'session stderr' in output()))
    check('logs-render-literally',qt('state')['richUserLabels']==0)
    qt('key',sequence='Escape');wait(lambda:not visible('Captured session output','text'))
    check('logs-escape-returns-library-focus',wait(lambda:focused_name('Owned native fixture')))
    open_game('Owned native fixture')
    configure=find('Configure','button');play=find('Play','button');remove=find('Remove from library','button')
    check('active-profile-edit-and-relaunch-disabled',all(n is not None and not n.get_state_set().contains(Atspi.StateType.ENABLED) for n in (configure,play,remove)))
    library()
    qt('close');wait(lambda:visible('Keep open','button'))
    check('close-dialog-focuses-keep-open',wait(lambda:focused_name('Keep open')))
    press('Keep open');wait(lambda:not visible('Keep open','button'))
    check('close-keep-open-preserves-session',state()=='running' and (ROOT/'game-pid').exists())
    for _ in range(5):qt('recreate');wait(lambda:find('Owned native fixture','list item'))
    press('Sessions and logs');wait(lambda:enabled('Stop'))
    check('view-recreation-reconnects-same-session',scalar('SELECT count(*) FROM launch_sessions')==2 and 'heartbeat' in wait(lambda:output()))
    qt('resize',width=980,height=660);qt('screenshot')
    (ROOT/'minimum.png').write_bytes((ROOT/'screenshot.png').read_bytes())
    check('session-minimum-geometry',qt('state')['width']==980)
    qt('resize',width=1440,height=900)
    close_panel();qt('close');wait(lambda:visible('Leave running and close','button'))
    press('Leave running and close');wait(lambda:find('Rustris','application') is None)
    check('leave-running-retains-helper-and-reservation',scalar('SELECT count(*) FROM session_reservations')==1 and state()=='unknown')
    launch_app();press('Sessions and logs');wait(lambda:enabled('Stop'))
    check('desktop-restart-reattaches-without-relaunch',state()=='running' and scalar('SELECT count(*) FROM launch_sessions')==2)
    log_path=ROOT/'state/rustris/logs'/str(__import__('uuid').UUID(bytes=scalar('SELECT id FROM launch_sessions ORDER BY created_ms DESC LIMIT 1')))
    log_path=log_path.with_suffix('.log')
    size=log_path.stat().st_size
    children[-1].kill();children[-1].wait(timeout=5)
    wait(lambda:find('Rustris','application') is None)
    wait(lambda:log_path.stat().st_size>size)
    check('actual-desktop-crash-keeps-capture-and-reservation',scalar('SELECT count(*) FROM session_reservations')==1)
    launch_app();press('Sessions and logs');wait(lambda:enabled('Stop'))
    check('actual-desktop-crash-reconnects-original-session',state()=='running' and scalar('SELECT count(*) FROM launch_sessions')==2)
    press('Stop');wait(lambda:state()=='stop_requested');wait(lambda:enabled('Force stop…'))
    check('stop-intent-visible-before-explicit-force',not enabled('Stop') and enabled('Force stop…'))
    press('Force stop…');wait(lambda:visible('Force stop','button'))
    check('force-dialog-focuses-cancel',wait(lambda:focused_name('Cancel')));qt('key',sequence='Space')
    check('force-confirmation-cancel-keeps-scope',state()=='stop_requested')
    press('Force stop…');wait(lambda:visible('Force stop','button'));press('Force stop')
    wait(lambda:state()=='stopped');wait(lambda:'heartbeat' in output())
    check('explicit-force-observed-terminal-and-history',wait(lambda:scalar('SELECT count(*) FROM session_reservations')==0 and any('root signal: 9' in n.get_name() for n in nodes())))
    close_panel();qt('close');wait(lambda:find('Rustris','application') is None)
    launch_app();press('Sessions and logs');wait(lambda:'session stdout' in output())
    check('completed-log-reopens-after-desktop-restart','stderr' in output() and 'stdout' in output())
    qt('screenshot');close_panel();qt('close');wait(lambda:find('Rustris','application') is None)
    saved_log=ROOT/'saved-completed-session.log'
    log_path.rename(saved_log)
    os.mkfifo(log_path,0o600)
    try:
        launch_app();press('Sessions and logs')
        check('completed-fifo-log-shows-error',wait(lambda:any('owned private bounded file' in n.get_name() for n in nodes())))
        check('completed-fifo-log-keeps-ui-responsive',not qt('state').get('error'))
        press('Refresh')
        check('completed-fifo-log-refresh-finishes',wait(lambda:enabled('Refresh') and any('owned private bounded file' in n.get_name() for n in nodes())))
        close_panel()
    finally:
        log_path.unlink(missing_ok=True)
        saved_log.rename(log_path)
    cancelled=ROOT/'cancelled.py'
    cancelled.write_text("open('cancelled-spawned','w').write('unexpected')\n")
    add('Cancelled native fixture','/usr/bin/python3',[str(cancelled)])
    lock=db();lock.execute('BEGIN IMMEDIATE')
    # Preparation waits only 250 ms for a locked library, shorter than one walk of
    # the accessibility tree. Locate the hidden Cancel buttons first, then poll only
    # their showing state and press the one that appears.
    cancels=[n for n in nodes() if n.get_name()=='Cancel preparation' and n.get_role_name() in ('push button','button')]
    press('Play')
    shown=wait(lambda:next((n for n in cancels if n.get_state_set().contains(Atspi.StateType.SHOWING)),None),timeout=5)
    action=shown.get_action_iface();assert any(action.get_action_name(i).casefold() in ('press','click') and action.do_action(i) for i in range(action.get_n_actions()))
    lock.rollback();lock.close()
    wait(lambda:state()=='cancelled')
    check('ui-cancel-before-spawn-creates-no-helper-or-game',scalar('SELECT helper_pid FROM launch_sessions ORDER BY created_ms DESC LIMIT 1') is None and not (ROOT/'cancelled-spawned').exists() and scalar('SELECT count(*) FROM session_reservations')==0)
    close_panel()
    add('Fast exit fixture','/usr/bin/true')
    press('Play');wait(lambda:state()=='exited');wait(lambda:any('root exit: 0' in n.get_name() for n in nodes()))
    check('fast-exit-shown-without-playability-claim',scalar('SELECT count(*) FROM session_reservations')==0)
    close_panel()
    flood=ROOT/'flood.py'
    flood.write_text("import os,time\nfor i in range(2048): os.write(1,b'x'*32768)\nopen('flood-done','w').write('done')\ntime.sleep(15)\n")
    add('Flood native fixture','/usr/bin/python3',[str(flood)])
    press('Play');wait(lambda:state()=='running');wait(lambda:(ROOT/'flood-done').exists())
    wait(lambda:any('display tail was clipped' in n.get_name() for n in nodes()))
    report['metrics']['flood_accessible_text_characters']=len(output())
    report['metrics']['flood_accessible_text_utf8_bytes']=len(output().encode())
    report['metrics']['flood_stop_enabled']=enabled('Stop')
    print('FLOOD '+json.dumps(report['metrics']),flush=True)
    check('flood-displays-clipping-and-bounded-text',wait(lambda:len(output())<=128*1024 and enabled('Stop')))
    began=time.monotonic();qt('state');report['metrics']['flood_qt_roundtrip_ms']=round((time.monotonic()-began)*1000,2)
    check('flood-ui-stays-responsive',report['metrics']['flood_qt_roundtrip_ms']<500)
    import signal
    helper=scalar('SELECT helper_pid FROM launch_sessions ORDER BY created_ms DESC LIMIT 1')
    descriptor=os.pidfd_open(helper);signal.pidfd_send_signal(descriptor,signal.SIGKILL);os.close(descriptor)
    wait(lambda:state()=='unknown');wait(lambda:visible('Release unknown…','button'))
    check('helper-loss-is-unknown-and-reserved',scalar('SELECT count(*) FROM session_reservations')==1 and not enabled('Stop'))
    press('Release unknown…');wait(lambda:visible('Release reservation','button'))
    check('release-dialog-focuses-cancel',wait(lambda:focused_name('Cancel')));press('Cancel')
    check('release-warning-cancel-retains-reservation',scalar('SELECT count(*) FROM session_reservations')==1)
    press('Release unknown…');wait(lambda:visible('Release reservation','button'));press('Release reservation')
    wait(lambda:state()=='released_unknown')
    check('explicit-release-records-uncertain-outcome',scalar('SELECT count(*) FROM session_reservations')==0)
    close_panel()
    streaming=ROOT/'streaming.py'
    streaming.write_text("import os,time\nchunk=b'x'*32768\nfor i in range(2048):\n os.write(1,chunk)\n if i==127: open('streaming-ready','w').write('ready')\n time.sleep(.01)\nopen('streaming-complete','w').write('done')\n")
    add('Streaming stop fixture','/usr/bin/python3',[str(streaming)])
    press('Play');wait(lambda:state()=='running');wait(lambda:(ROOT/'streaming-ready').exists())
    wait(lambda:enabled('Stop'))
    check('streaming-output-still-active-before-stop',not (ROOT/'streaming-complete').exists())
    started=time.monotonic();press('Stop');report['metrics']['stream_stop_action_ms']=round((time.monotonic()-started)*1000,2)
    wait(lambda:state()=='stopped')
    completion=json.loads(scalar('SELECT detail FROM launch_sessions ORDER BY created_ms DESC LIMIT 1'))
    check('ui-stop-during-saturated-output',report['metrics']['stream_stop_action_ms']<1000 and not (ROOT/'streaming-complete').exists() and completion['logs']['stdout_read']>=4*1024*1024 and completion['logs']['memory_dropped']>0 and scalar('SELECT count(*) FROM session_reservations')==0)
    close_panel();qt('close');wait(lambda:find('Rustris','application') is None)
    wait(lambda:children[-1].poll() is not None)
    check('session-normal-close-zero-exit',children[-1].returncode==0)
    report['passed']=True
except Exception as error:
    report['passed']=False;report['error']=str(error);report['tree']=[]
    for n in nodes():
        try:report['tree'].append({'name':n.get_name(),'role':n.get_role_name()})
        except gi.repository.GLib.Error:pass
    raise
finally:
    (ROOT/'report.json').write_text(json.dumps(report,indent=2))
    for process in children:
        if process.poll() is None:
            process.terminate()
            try:process.wait(timeout=5)
            except subprocess.TimeoutExpired:process.kill();process.wait()
    for log in logs:log.close()
