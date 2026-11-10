#!/usr/bin/python3
"""Native regression suite against the real QML/worker/storage application.

Runs only inside desktop_session.py's private offline namespace. AT-SPI checks
semantic behavior; QtTest supplies deterministic keys, resizing and capture.
"""
import json
import os
from pathlib import Path
import sqlite3
import subprocess
import sys
import time
import uuid
import gi
gi.require_version('Atspi','2.0')
from gi.repository import Atspi
ROOT=Path(sys.argv[1]);BINARY=Path(sys.argv[2]);SCALE=float(sys.argv[3])
report={'scale':SCALE,'checks':[],'metrics':{}}
children=[];logs=[]

def wait(predicate, timeout=10):
    end=time.monotonic()+timeout
    while time.monotonic()<end:
        try:value=predicate()
        except gi.repository.GLib.Error:value=None
        if value:return value
        time.sleep(.025)
    raise AssertionError('Timed out: '+str(predicate))

def walk(node,depth=0):
    if depth>18 or node is None:return
    try:
        node.get_name()
        node.get_role_name()
        yield node
        for i in range(node.get_child_count()):yield from walk(node.get_child_at_index(i),depth+1)
    except gi.repository.GLib.Error:return

def nodes():return list(walk(Atspi.get_desktop(0)))
def find(name,role=None):
    return next((n for n in nodes() if n.get_name()==name and (role is None or n.get_role_name()==role)),None)
def visible(name,role=None):
    return next((n for n in nodes() if n.get_name()==name and (role is None or n.get_role_name()==role) and n.get_state_set().contains(Atspi.StateType.SHOWING)),None)
def press(name,role='push button'):
    def locate():
        node=visible(name,role) or visible(name,'button') if role=='push button' else visible(name,role)
        if node:return node
        # Pages scroll a focused control into view; bring an off-screen control forward.
        # Several pages can share a control name, so try each enabled match: Qt refuses
        # focus for controls on pages that are not shown.
        roles=(role,'button') if role=='push button' else (role,)
        for hidden in (n for n in nodes() if n.get_name()==name and n.get_role_name() in roles):
            if hidden.get_state_set().contains(Atspi.StateType.ENABLED):
                component=hidden.get_component_iface()
                if component is not None and component.grab_focus():
                    break
        return None
    node=wait(locate)
    iface=node.get_action_iface();assert iface is not None,name
    for i in range(iface.get_n_actions()):
        if iface.get_action_name(i).casefold() in ('press','click','toggle'):
            assert iface.do_action(i),name
            return
    raise AssertionError('No native press: '+name)
def text(name,value):
    node=wait(lambda:find(name,'text'))
    assert node.get_editable_text_iface().set_text_contents(value),name
    wait(lambda:Atspi.Text.get_text(node,0,-1)==value)
def check(name,value):
    assert value,name
    report['checks'].append(name)
    print('PASS '+name,flush=True)
def qt(action,**values):
    data={'id':str(uuid.uuid4()),'action':action,**values}
    temp=ROOT/'qt-request.tmp';temp.write_text(json.dumps(data));temp.rename(ROOT/'qt-request.json')
    def response():
        path=ROOT/'qt-response.json'
        if not path.exists():return None
        answer=json.loads(path.read_text())
        return answer if answer['id']==data['id'] else None
    answer=wait(response)
    assert not answer['error'],answer
    return answer

def checked(name):
    node=find(name,'check box')
    return node is not None and node.get_state_set().contains(Atspi.StateType.CHECKED)
def focused_name(name):
    return any(n.get_name()==name and n.get_state_set().contains(Atspi.StateType.FOCUSED) for n in nodes())
def db():return sqlite3.connect(ROOT/'data/rustris/library.sqlite3',timeout=1)
def scalar(sql):
    with db() as connection:return connection.execute(sql).fetchone()[0]
def restart():
    qt('close')
    wait(lambda:find('Rustris','application') is None)
    (ROOT/'qt-request.json').unlink(missing_ok=True)
    log=(ROOT/f'restart-{len(logs)}.log').open('w');logs.append(log)
    started=time.monotonic()
    child=subprocess.Popen([str(BINARY)],stdout=log,stderr=subprocess.STDOUT);children.append(child)
    wait(lambda:find('Search games','text'))
    report['metrics']['restart_search_ready_ms']=round((time.monotonic()-started)*1000,2)
    wait(lambda:scalar('SELECT count(*) FROM games')>0)

def escaped(raw):
    return ''.join(chr(b) if 32<=b<127 and b!=92 else ('\\\\' if b==92 else f'\\x{b:02x}') for b in raw)

def library():
    """Return to the library page from any page."""
    press('Library')
    wait(lambda:visible('Filter library','text'))
def reveal(name,role='button'):
    """Focus a control below the fold so its page scrolls it into view, then return it."""
    node=wait(lambda:find(name,role) or (find(name,'push button') if role=='button' else None))
    assert node.get_component_iface().grab_focus(),name
    return wait(lambda:visible(name,role) or (visible(name,'push button') if role=='button' else None))
def open_game(name):
    """Open a game's page from the library, returning to the library first if needed."""
    if not visible(name,'list item'):library()
    press(name,'list item')
    wait(lambda:visible('Configure','button') or visible('Configure','push button'))
def press_revealed(name,role='push button'):
    reveal(name,'button' if role=='push button' else role)
    press(name,role)
def runner_paths():
    """Show the Runners page's manual path fields.

    D-012: when Proton builds are detected (the host's /usr/share builds are
    visible even in the private desktop) the fields sit behind Advanced settings.
    """
    wait(lambda:visible('Advanced settings','check box') or visible('UMU executable','text'))
    if not visible('UMU executable','text'):
        press('Advanced settings','check box')
    wait(lambda:visible('UMU executable','text'))
