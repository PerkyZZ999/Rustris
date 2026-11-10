#!/usr/bin/python3
"""Default copied build artwork smoke, using only AT-SPI and private fixture data."""
import json, os, subprocess, sqlite3, struct, sys, time, uuid, zlib
from pathlib import Path
root=Path(sys.argv[1])
image=root/'home/art.png'
database=root/'data/rustris/library.sqlite3'
if len(sys.argv)>2 and sys.argv[2]=='--prepare':
    def chunk(kind,data):return struct.pack('>I',len(data))+kind+data+struct.pack('>I',zlib.crc32(kind+data))
    image.write_bytes(b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',1,1,8,2,0,0,0))+chunk(b'IDAT',zlib.compress(b'\0\xff\0\0'))+chunk(b'IEND',b''))
    fresh=root/'home/fresh.png'
    fresh.write_bytes(b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',1,1,8,2,0,0,0))+chunk(b'IDAT',zlib.compress(b'\0\x14\x3c\xe6'))+chunk(b'IEND',b''))
    database.parent.mkdir(mode=0o700)
    connection=sqlite3.connect(database)
    source=Path(__file__).resolve().parents[1]/'crates/rustris-core/src'
    for schema in ('schema.sql','schema002.sql','schema003.sql','schema004.sql'):connection.executescript((source/schema).read_text())
    connection.execute('PRAGMA application_id=1381323604');connection.execute('PRAGMA user_version=4')
    game,installation,profile=(uuid.uuid4().bytes for _ in range(3))
    connection.execute('INSERT INTO games(id,display_name) VALUES(?,?)',(game,'Copied release artwork'))
    connection.execute('INSERT INTO installations VALUES(?,?)',(installation,game))
    connection.execute("INSERT INTO profiles(id,installation_id,executable,cwd,runner) VALUES(?,?,?,?,?)",(profile,installation,b'/usr/bin/true',b'/', 'native'))
    connection.execute('UPDATE ui_state SET selected_game=?',(game,))
    connection.execute('INSERT INTO local_artwork VALUES(?,?)',(game,os.fsencode(image)))
    connection.commit();connection.close();database.chmod(0o600)
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
def find(name,role=None,showing=True):
    return next((n for n in walk(Atspi.get_desktop(0)) if n.get_name()==name and (role is None or n.get_role_name()==role) and (not showing or n.get_state_set().contains(Atspi.StateType.SHOWING))),None)
def wait(predicate):
    deadline=time.monotonic()+20
    while time.monotonic()<deadline:
        value=predicate()
        if value:return value
        time.sleep(.03)
    raise AssertionError('Expected native release state not observed')
def press(name,role='button'):
    # Prefer the on-screen control; pages keep hidden controls with the same names.
    node=wait(lambda:find(name,role,True) or find(name,role,False))
    action=node.get_action_iface();assert action,name
    for i in range(action.get_n_actions()):
        if action.get_action_name(i).casefold() in ('press','click','toggle','activate'):
            assert action.do_action(i),name
            return
    raise AssertionError('Native action unavailable: '+name)
report={'passed':False,'checks':[]}
try:
    press('Copied release artwork','list item')
    wait(lambda:find('Local artwork selected.','label'));report['checks'].append('copied-release-renders-stored-artwork')
    choice=wait(lambda:find('Choose artwork…','button',False))
    assert choice.get_component_iface().grab_focus()
    def choice_visible():
        bounds=choice.get_component_iface().get_extents(Atspi.CoordType.WINDOW)
        return choice.get_state_set().contains(Atspi.StateType.FOCUSED) and bounds.y>=56 and bounds.y+bounds.height<=900
    wait(choice_visible);report['checks'].append('copied-release-reveals-focused-artwork-choice')
    press('Settings')
    wait(lambda:find('Opaque panels','check box') and find('Opaque panels','check box').get_state_set().contains(Atspi.StateType.FOCUSED))
    report['checks'].append('copied-release-settings-initial-focus')
    press('Done')
    press('Choose artwork…');wait(lambda:find('Parent folder','button'))
    press('fresh.png')
    def picked():
        with sqlite3.connect(database) as c:
            row=c.execute('SELECT path FROM local_artwork').fetchone()
            return row is not None and row[0]==os.fsencode(root/'home/fresh.png')
    wait(picked);wait(lambda:find('Local artwork selected.','label') and find('Parent folder','button') is None)
    assert (root/'home/fresh.png').exists();report['checks'].append('copied-release-picker-selects-file')
    press('Choose artwork…');wait(lambda:find('Parent folder','button'));press('Cancel')
    report['checks'].append('copied-release-picker-opens-and-cancels')
    press('Clear artwork')
    def cleared():
        with sqlite3.connect(database) as c:return c.execute('SELECT count(*) FROM local_artwork').fetchone()[0]==0
    wait(cleared);wait(lambda:find('No local artwork selected.','label'))
    assert image.exists();report['checks'].append('copied-release-clear-preserves-file')
    press('Enter artwork path…')
    field=wait(lambda:find('Artwork path','text',False))
    assert field.get_editable_text_iface().set_text_contents(str(image))
    press('Save artwork path')
    def selected():
        with sqlite3.connect(database) as c:
            row=c.execute('SELECT path FROM local_artwork').fetchone()
            return row is not None and row[0]==os.fsencode(image)
    wait(selected);wait(lambda:find('Local artwork selected.','label'))
    report['checks'].append('copied-release-selects-artwork-by-path')
    press('Enter artwork path…')
    field=wait(lambda:find('Artwork path','text',False))
    assert field.get_editable_text_iface().set_text_contents(r'/missing/new_\xff.png')
    press('Artwork path uses exact Unix bytes','check box');press('Save artwork path')
    def exact_saved():
        with sqlite3.connect(database) as c:
            row=c.execute('SELECT path FROM local_artwork').fetchone()
            return row is not None and row[0]==b'/missing/new_\xff.png'
    wait(exact_saved);wait(lambda:find('Artwork unavailable or unreadable. The original selection is preserved.','label'))
    report['checks'].append('copied-release-exact-artwork-path')
    assert not (root/'qt-response.json').exists();report['checks'].append('copied-release-has-no-test-transport')
    report['passed']=True
finally:
    if not report['passed']:
        subprocess.run(['spectacle','--background','--fullscreen','--nonotify','--output',str(root/'failure.png')],capture_output=True,timeout=30)
        report['labels']=[(n.get_name(),n.get_role_name(),n.get_state_set().contains(Atspi.StateType.SHOWING)) for n in walk(Atspi.get_desktop(0)) if n.get_role_name() in ('label','push button','button','dialog')][:200]
    (root/'release-artwork-report.json').write_text(json.dumps(report,indent=2))
