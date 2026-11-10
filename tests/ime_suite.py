#!/usr/bin/python3
"""Composed text through the compositor keymap in a copied default build.

desktop_session.py gives its private KWin a US international (dead-key)
keymap. Keys travel through the compositor via EIS. With `default`, Qt uses
whatever composition it selects without an input method; with `ibus`, a private
ibus-daemon runs the xkb:us:intl:eng engine and Qt's ibus module commits the
text. Neither is a physical keyboard or a CJK input method.
"""
import json
from pathlib import Path
import subprocess
import sys
import time
from native_support import ROOT, Atspi, report, wait, find, press, check, db
from eis_input import Input

MODULE = sys.argv[4]
NAME = 'Café Ünï Àpp'
PATH = '/opt/Café/gáme'


def value(name):
    node = find(name, 'text')
    return Atspi.Text.get_text(node, 0, -1) if node else None


def enter(device, name, strokes):
    node = wait(lambda: find(name, 'text'))
    assert node.get_component_iface().grab_focus(), name
    wait(lambda: find(name, 'text').get_state_set().contains(Atspi.StateType.FOCUSED))
    device.strokes(strokes)
    time.sleep(.5)
    return value(name)


try:
    wait(lambda: find('Search games', 'text'), timeout=30)
    report['metrics']['input_module'] = MODULE
    if MODULE == 'ibus':
        engine = subprocess.run(['ibus', 'engine'], capture_output=True, text=True, timeout=10)
        report['metrics']['ibus_engine'] = engine.stdout.strip()
        check('private-ibus-engine-active', engine.stdout.strip() == 'xkb:us:intl:eng')
    device = Input()
    press('Add game'); wait(lambda: find('Game name', 'text'))
    typed_name = enter(device, 'Game name', 'Caf\'e "Un"i `App')
    report['metrics']['name_field'] = typed_name
    check('dead-key-composition-in-name-field', typed_name == NAME)
    typed_path = enter(device, 'Executable path', '/opt/Caf\'e/g\'ame')
    report['metrics']['path_field'] = typed_path
    check('dead-key-composition-in-path-field', typed_path == PATH)
    press('Save profile')
    wait(lambda: find(NAME, 'list item'), timeout=30)
    with db() as connection:
        saved = connection.execute('SELECT games.display_name, profiles.executable FROM games JOIN installations ON '
                                   'installations.game_id=games.id JOIN profiles ON profiles.installation_id=installations.id').fetchone()
    report['metrics']['saved'] = [saved[0], bytes(saved[1]).hex()]
    application = Path(sys.argv[2]).resolve()
    plugins = set()
    for proc in Path('/proc').iterdir():
        try:
            if proc.name.isdigit() and Path(proc/'exe').resolve() == application:
                plugins |= {line.split()[-1].rsplit('/', 1)[-1] for line in (proc/'maps').read_text().splitlines()
                            if 'platforminputcontexts' in line}
        except OSError:
            continue
    report['metrics']['input_context_plugins'] = sorted(plugins)
    if MODULE == 'ibus':
        check('qt-ibus-input-context-loaded', any('ibus' in name for name in plugins))
    check('composed-name-saved-as-utf8', saved[0] == NAME)
    check('composed-path-saved-as-exact-utf8-bytes', bytes(saved[1]) == PATH.encode())
    report['passed'] = True
except Exception as error:
    report['passed'] = False
    report['error'] = str(error)
    raise
finally:
    (ROOT/'report.json').write_text(json.dumps(report, indent=2)+'\n')
