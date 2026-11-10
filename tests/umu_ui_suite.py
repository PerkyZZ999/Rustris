#!/usr/bin/python3
"""Exercise UMU review using only the private Windows fixture and copied runners."""
from native_support import ROOT,SCALE,Atspi,report,wait,find,visible,press,text,qt,check,scalar,nodes,json,sys,time,focused_name, runner_paths
from pathlib import Path

inputs=Path(sys.argv[4])
runtime='steamrt4' if (inputs/'data/umu/steamrt4').is_dir() else 'steamrt3'
prefix=ROOT/'wine-prefix';prefix.mkdir()

def enabled(name):
    node=visible(name,'button') or visible(name,'push button')
    return node is not None and node.get_state_set().contains(Atspi.StateType.ENABLED)
def state():
    from native_support import db
    with db() as connection:
        row=connection.execute('SELECT state FROM launch_sessions ORDER BY created_ms DESC LIMIT 1').fetchone()
        return row[0] if row else None

try:
    wait(lambda:find('Search games','text'))
    press('Settings');press('Runner settings');runner_paths()
    for name,value in [('UMU executable','/usr/bin/umu-run'),('Proton directory',str(inputs/'proton')),('Prepared runtime directory',str(inputs/'data/umu'/runtime))]:text(name,value)
    press('Save runner settings');wait(lambda:not visible('Save runner settings','button'))
    check('runner-selection-persisted',(ROOT/'config/rustris/runners.toml').exists())
    press('Add game');text('Game name','Windows fixture');text('Executable path',str(inputs/'windows-fixture.exe'));text('Working directory',str(ROOT))
    press('UMU / Proton','radio button')
    text('Wine prefix',str(prefix))
    for i,arg in enumerate(['Z:'+str(ROOT/'windows.json').replace('/','\\'),'hold','8000','spaces and 🦀','$(touch never)']):
        press('Add argument');text(f'Argument {i+1}',arg)
    press('Save profile');wait(lambda:enabled('Review and play'))
    press('Review and play');wait(lambda:visible('Launch game','button'),timeout=30)
    check('review-focuses-cancel',wait(lambda:focused_name('Cancel')))
    qt('key',sequence='Ctrl+N');qt('key',sequence='Ctrl+F')
    check('review-blocks-global-shortcuts',focused_name('Cancel') and not visible('Save profile','button'))
    check('review-ready-and-evidence-visible',enabled('Launch game') and any('Evidence:' in n.get_name() for n in nodes()))
    if runtime=='steamrt4':
        check('ge-and-steamrt4-versions-visible',any('GE-Proton11-5' in n.get_name() and '4.0.20261021.265997' in n.get_name() for n in nodes()))
    check('review-does-not-start-process',scalar('SELECT count(*) FROM launch_sessions')==0 and not (ROOT/'windows.json').exists() and not (prefix/'drive_c').exists())
    qt('key',sequence='Space');wait(lambda:not visible('Launch game','button'))
    check('review-cancel-has-no-session',scalar('SELECT count(*) FROM launch_sessions')==0)
    press('Review and play');wait(lambda:enabled('Launch game'),timeout=30);press('Launch game')
    check('launch-opens-session-with-focus',wait(lambda:focused_name('Choose session')))
    wait(lambda:(ROOT/'windows.json').exists(),timeout=60)
    result=json.loads((ROOT/'windows.json').read_text())
    check('fixture-executed-exact-arguments',result['argv'][4:]==['spaces and 🦀','$(touch never)'] and not (ROOT/'never').exists())
    check('umu-output-coverage-visible',wait(lambda:any('Windows application console output is unverified' in n.get_name() for n in nodes())))
    wait(lambda:enabled('Stop'),timeout=20);press('Stop');wait(lambda:state()=='stopped',timeout=30)
    check('umu-owned-stop-completes',scalar('SELECT count(*) FROM session_reservations')==0)
    # D-010: the approved, unchanged review launches directly with Play.
    check('approval-recorded',scalar('SELECT count(*) FROM approved_reviews')==1)
    check('approved-review-offers-play',wait(lambda:enabled('Play') and not visible('Review and play','button'),timeout=30))
    (ROOT/'windows.json').unlink()
    press('Play')
    def second_launch():
        assert not visible('Launch game','button'),'review dialog shown for an unchanged approved review'
        return scalar('SELECT count(*) FROM launch_sessions')==2
    check('unchanged-review-launches-without-dialog',wait(second_launch,timeout=60))
    wait(lambda:(ROOT/'windows.json').exists(),timeout=60)
    wait(lambda:enabled('Stop'),timeout=20);press('Stop');wait(lambda:state()=='stopped',timeout=30)
    # Any change to what was reviewed asks again.
    press('Configure');text('Argument 3','8001');press('Save profile')
    check('changed-profile-asks-for-review',wait(lambda:enabled('Review and play'),timeout=30))
    press('Review and play');wait(lambda:visible('Launch game','button'),timeout=30)
    check('changed-review-is-shown',scalar('SELECT count(*) FROM launch_sessions')==2)
    press('Cancel');wait(lambda:not visible('Launch game','button'))
    report['passed']=True
except Exception as error:
    report['passed']=False;report['error']=str(error);report['tree']=[]
    for node in nodes():
        try:report['tree'].append({'name':node.get_name(),'role':node.get_role_name()})
        except Exception:pass
    raise
finally:
    (ROOT/'report.json').write_text(json.dumps(report,indent=2))
