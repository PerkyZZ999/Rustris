#!/usr/bin/python3
"""Copied default desktop/helper pair: real Play/capture/Stop without QtTest."""
from native_support import ROOT,BINARY,report,wait,find,press,text,check,db,scalar,Atspi,json,time

def state():
    with db() as c:
        row=c.execute('SELECT state FROM launch_sessions ORDER BY created_ms DESC LIMIT 1').fetchone()
        return row[0] if row else None
try:
    wait(lambda:find('Search games','text'))
    check('default-copied-desktop-loads-outside-checkout',True)
    fixture=ROOT/'release.py'
    fixture.write_text("import sys,time\nprint('default release stdout',flush=True)\nprint('default release stderr',file=sys.stderr,flush=True)\ntime.sleep(15)\n")
    press('Add game');wait(lambda:find('Game name','text'))
    text('Game name','Default release native fixture');text('Executable path','/usr/bin/python3');text('Working directory',str(ROOT))
    press('Add argument');text('Argument 1',str(fixture));press('Save profile')
    wait(lambda:find('Default release native fixture','list item'))
    check('default-copied-desktop-persists-profile',scalar('SELECT executable FROM profiles')==b'/usr/bin/python3')
    press('Play');wait(lambda:state()=='running')
    check('copied-companion-helper-starts-real-owned-process',scalar('SELECT helper_pid FROM launch_sessions') is not None and scalar('SELECT count(*) FROM session_reservations')==1)
    def output():
        node=find('Captured session output','text')
        return Atspi.Text.get_text(node,0,-1) if node else ''
    wait(lambda:'default release stdout' in output() and 'default release stderr' in output())
    check('default-release-captures-both-streams',True)
    press('Stop');wait(lambda:state()=='stopped');wait(lambda:scalar('SELECT count(*) FROM session_reservations')==0)
    check('default-release-owned-stop-releases-after-observation',True)
    (ROOT/'qt-request.json').write_text(json.dumps({'id':'default-must-ignore','action':'state'}));time.sleep(.1)
    check('default-pair-has-no-test-command-transport',not (ROOT/'qt-response.json').exists())
    report['passed']=True
except Exception as error:
    report['passed']=False;report['error']=str(error);raise
finally:
    (ROOT/'report.json').write_text(json.dumps(report,indent=2))
