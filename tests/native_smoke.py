#!/usr/bin/python3
"""Default release smoke: no QtTest dependency or test command transport."""
import json
from pathlib import Path
import sqlite3
import sys
import time
import gi
gi.require_version('Atspi','2.0')
from gi.repository import Atspi

root=Path(sys.argv[1])
def walk(node):
    if node is None:return
    yield node
    for index in range(node.get_child_count()):yield from walk(node.get_child_at_index(index))
def find(name,role=None):
    return next((node for node in walk(Atspi.get_desktop(0)) if node.get_name()==name and (role is None or node.get_role_name()==role)),None)
def wait(predicate):
    deadline=time.monotonic()+15
    while time.monotonic()<deadline:
        value=predicate()
        if value:return value
        time.sleep(.025)
    raise AssertionError('Release state was not observed')
def press(name):
    node=wait(lambda:find(name,'button'))
    action=node.get_action_iface()
    assert action is not None
    for index in range(action.get_n_actions()):
        if action.get_action_name(index).casefold() in ('press','click'):
            assert action.do_action(index)
            return
    raise AssertionError('Release action unavailable')

report={'passed':False,'checks':[]}
try:
    started=time.monotonic()
    wait(lambda:find('Search games','text'))
    report['checks'].append('native-release-library-loaded')
    press('Add game')
    for name,value in [('Game name','Release fixture 🦀'),('Executable path','/unavailable/release fixture')]:
        field=wait(lambda:find(name,'text'))
        assert field.get_editable_text_iface().set_text_contents(value)
        wait(lambda:Atspi.Text.get_text(field,0,-1)==value)
    for _ in range(6):press('Add argument')
    argument=wait(lambda:find('Argument 6','text'))
    assert argument.get_component_iface().grab_focus()
    def argument_visible():
        frame=wait(lambda:find('Rustris — Library','frame')).get_component_iface().get_extents(Atspi.CoordType.SCREEN)
        bounds=argument.get_component_iface().get_extents(Atspi.CoordType.SCREEN)
        y=bounds.y-frame.y
        return argument.get_state_set().contains(Atspi.StateType.FOCUSED) and y>=56 and y+bounds.height<=frame.height
    wait(argument_visible)
    report['checks'].append('native-release-scrolls-focused-argument-into-view')
    press('Save profile')
    wait(lambda:find('Release fixture 🦀','list item'))
    with sqlite3.connect(root/'data/rustris/library.sqlite3') as connection:
        row=connection.execute('SELECT g.display_name,p.executable,p.cwd FROM games g JOIN installations i ON i.game_id=g.id JOIN profiles p ON p.installation_id=i.id').fetchone()
    assert row==('Release fixture 🦀',b'/unavailable/release fixture',b'/unavailable'),row
    report['checks'].append('native-release-save-persists-exact-profile')
    assert not (root/'qt-response.json').exists()
    report['checks'].append('default-release-does-not-read-test-transport')
    report['elapsed_with_native_observation_ms']=round((time.monotonic()-started)*1000,2)
    report['passed']=True
finally:
    (root/'release-report.json').write_text(json.dumps(report,indent=2))
