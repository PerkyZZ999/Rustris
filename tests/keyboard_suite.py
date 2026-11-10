#!/usr/bin/python3
"""Keyboard-driven primary navigation in an isolated desktop test build."""
from native_support import ROOT,Atspi,report,wait,find,visible,qt,check,scalar,nodes,json,focused_name,sys
import shutil


def focus(name,sequence='Tab',limit=120):
    for _ in range(limit):
        if focused_name(name):
            return
        qt('key',sequence=sequence)
    raise AssertionError('Keyboard focus did not reach '+name)


def activate(name):
    focus(name)
    qt('key',sequence='Space')


def field(name,value):
    focus(name)
    qt('type-ascii',text=value)
    assert Atspi.Text.get_text(find(name,'text'),0,-1)==value

def screenshot(name):
    qt('screenshot')
    shutil.copyfile(ROOT/'screenshot.png',ROOT/name)

def within_window(name,height,role='button'):
    """The focused control is fully inside the window below the title bar."""
    node=visible(name,role) or (visible(name,'push button') if role=='button' else None)
    frame=visible('Rustris — Library','frame')
    if not node or not frame:return False
    bounds=node.get_component_iface().get_extents(Atspi.CoordType.SCREEN)
    origin=frame.get_component_iface().get_extents(Atspi.CoordType.SCREEN)
    y=bounds.y-origin.y
    return y>=56 and bounds.height>0 and y+bounds.height<=height

try:
    wait(lambda:find('Search games','text'))
    width,height=map(int,sys.argv[4].split('x'))
    qt('resize',width=width,height=height)
    geometry=qt('state')
    check('keyboard-initial-window-size',geometry['width']==width and geometry['height']==height)
    qt('key',sequence='Ctrl+N')
    wait(lambda:focused_name('Game name'))
    screenshot('profile-editor.png')
    field('Game name','Keyboard fixture')
    field('Executable path','/usr/bin/true')
    focus('Add argument')
    for index in range(6):
        qt('key',sequence='Space')
        wait(lambda:find(f'Argument {index+1}','text'))
    focus('Argument 6')
    screenshot('argument-focus.png')
    check('keyboard-last-argument-visible',wait(lambda:within_window('Argument 6',height,'text')))
    field('Argument 6','--test')
    activate('Add variable')
    field('Environment key 1','TEST_VALUE')
    check('keyboard-environment-key-visible',wait(lambda:within_window('Environment key 1',height,'text')))
    field('Environment value 1','value')
    check('keyboard-environment-value-visible',wait(lambda:within_window('Environment value 1',height,'text')))
    focus('Unset variable 1')
    check('keyboard-environment-control-visible',wait(lambda:within_window('Unset variable 1',height,'check box')))
    focus('Game name','Shift+Tab')
    screenshot('profile-reverse-focus.png')
    check('keyboard-profile-reverse-navigation-visible',wait(lambda:within_window('Game name',height,'text')))
    qt('key',sequence='Ctrl+S')
    wait(lambda:scalar('SELECT count(*) FROM games')==1)
    check('keyboard-profile-arguments-and-environment-saved',scalar('SELECT value FROM arguments WHERE position=5')==b'--test' and scalar('SELECT value FROM environment')==b'value')
    check('keyboard-create-profile-focuses-play',wait(lambda:focused_name('Play')))
    qt('key',sequence='Ctrl+F');wait(lambda:focused_name('Search games'))
    qt('type-ascii',text='Keyboard')
    check('keyboard-search',wait(lambda:find('Keyboard fixture','list item')))
    qt('key',sequence='Down')
    check('keyboard-search-down-reaches-selection',wait(lambda:focused_name('Keyboard fixture')))
    screenshot('library.png')
    qt('key',sequence='Return')
    check('keyboard-enter-opens-game',wait(lambda:focused_name('Play')))
    focus('Choose artwork…')
    check('keyboard-artwork-choice-visible',wait(lambda:within_window('Choose artwork…',height)))
    screenshot('artwork-focus.png')
    check('keyboard-focuses-artwork-choice',focused_name('Choose artwork…'))
    focus('Enter artwork path…')
    check('keyboard-artwork-path-visible',wait(lambda:within_window('Enter artwork path…',height)))
    focus('Configure','Shift+Tab')
    check('keyboard-detail-reverse-navigation-visible',wait(lambda:within_window('Configure',height)))
    activate('Settings');wait(lambda:visible('Opaque panels','check box'))
    check('keyboard-settings-initial-focus',wait(lambda:focused_name('Opaque panels')))
    screenshot('settings-initial.png')
    activate('Opaque panels')
    check('keyboard-settings',find('Opaque panels','check box').get_state_set().contains(Atspi.StateType.CHECKED))
    activate('Done');wait(lambda:not visible('Opaque panels','check box'))
    activate('Play')
    wait(lambda:scalar('SELECT state FROM launch_sessions ORDER BY created_ms DESC LIMIT 1')=='exited',timeout=30)
    activate('Preflight evidence')
    check('keyboard-opens-preflight',wait(lambda:visible('Back to session','button')))
    qt('key',sequence='Ctrl+N');qt('key',sequence='Ctrl+F')
    check('preflight-blocks-global-shortcuts',focused_name('Back to session') and not visible('Save profile','button'))
    screenshot('preflight.png')
    activate('Back to session')
    wait(lambda:focused_name('Preflight evidence'))
    focus('Captured session output')
    check('keyboard-play-and-focus-logs',focused_name('Captured session output'))
    screenshot('session.png')
    qt('key',sequence='Escape')
    check('keyboard-logs-escape-returns-library-focus',wait(lambda:focused_name('Keyboard fixture')))
    qt('resize',width=1440,height=900)
    geometry=qt('state')
    check('keyboard-window-1440x900',geometry['width']==1440 and geometry['height']==900)
    activate('Settings');check('keyboard-settings-after-resize',wait(lambda:visible('Opaque panels','check box')))
    check('keyboard-settings-reopen-focus',wait(lambda:focused_name('Opaque panels')))
    screenshot('settings-1440.png')
    qt('key',sequence='Escape')
    check('keyboard-settings-escape-returns-library-focus',wait(lambda:focused_name('Keyboard fixture')))
    report['passed']=True
except Exception as error:
    report['passed']=False;report['error']=str(error);report['tree']=[]
    for node in nodes():
        try:report['tree'].append({'name':node.get_name(),'role':node.get_role_name(),'focused':node.get_state_set().contains(Atspi.StateType.FOCUSED)})
        except Exception:pass
    raise
finally:
    (ROOT/'report.json').write_text(json.dumps(report,indent=2))
