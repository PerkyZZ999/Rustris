#!/usr/bin/python3
"""Capture every main screen of the explicit test build for comparison with docs/mockup.

`--prepare` seeds a private schema-4 library before the application starts: a mix of
native and UMU profiles, favorites, a missing executable and a few generated covers.
The capture run then visits the screens with AT-SPI and QtTest and saves screenshots
under <root>/preview/. Nothing here inspects pixels; it only produces images to look at.
"""
import os
import sqlite3
import sys
import uuid
from pathlib import Path

root = Path(sys.argv[1])
GAMES = [
    ('Ashen Lighthouse', 'native', True, 'cover'), ('Copper Valley', 'native', True, None),
    ('Deep Signal', 'umu', False, 'cover'), ('Ember Trail', 'native', False, None),
    ('Frost Archive', 'umu', True, None), ('Glass Harbor', 'native', False, 'cover'),
    ('Hollow Orbit', 'native', False, None), ('Iron Meadow', 'umu', False, None),
    ('Juniper Rails', 'native', True, None), ('Kestrel Run', 'native', False, 'missing'),
    ('Lantern Deep', 'umu', False, None), ('Moss Circuit', 'native', False, None),
    ('Night Ferry', 'native', False, None), ('Opal Drift', 'native', False, None),
]
if len(sys.argv) == 3 and sys.argv[2] == '--prepare':
    from PIL import Image, ImageDraw
    database = root/'data/rustris/library.sqlite3'
    database.parent.mkdir(mode=0o700, parents=True)
    connection = sqlite3.connect(database)
    source = Path(__file__).resolve().parents[1]/'crates/rustris-core/src'
    for schema in ('schema.sql', 'schema002.sql', 'schema003.sql', 'schema004.sql'):
        connection.executescript((source/schema).read_text())
    connection.execute('PRAGMA application_id=1381323604')
    connection.execute('PRAGMA user_version=4')
    palettes = [((40, 70, 120), (230, 150, 70)), ((90, 30, 60), (250, 120, 80)), ((20, 80, 70), (160, 230, 200))]
    selected = None
    for number, (name, runner, favorite, extra) in enumerate(GAMES):
        game, installation, profile = (uuid.uuid4().bytes for _ in range(3))
        connection.execute('INSERT INTO games(id,display_name,favorite) VALUES(?,?,?)', (game, name, int(favorite)))
        connection.execute('INSERT INTO installations VALUES(?,?)', (installation, game))
        executable = b'/opt/games/absent/game.x86_64' if extra == 'missing' else (b'/usr/bin/true' if runner == 'native' else os.fsencode(root/'Game.exe'))
        if name == 'Ashen Lighthouse':
            # A short demo program so the Logs screenshot shows captured output.
            executable = os.fsencode(root/'ashen-lighthouse')
        prefix = os.fsencode(root/'prefix') if runner == 'umu' else None
        connection.execute('INSERT INTO profiles(id,installation_id,executable,cwd,runner,prefix) VALUES(?,?,?,?,?,?)',
                           (profile, installation, executable, os.fsencode(root), runner, prefix))
        if extra == 'cover':
            top, bottom = palettes[number % len(palettes)]
            image = Image.new('RGB', (400, 600))
            draw = ImageDraw.Draw(image)
            for y in range(600):
                t = y / 599
                draw.line([(0, y), (400, y)], fill=tuple(round(a + (b - a) * t) for a, b in zip(top, bottom)))
            draw.ellipse((120, 140, 280, 300), fill=(255, 220, 160))
            path = root/'home'/f'cover-{number}.png'
            image.save(path)
            connection.execute('INSERT INTO local_artwork VALUES(?,?)', (game, os.fsencode(path)))
        if name == 'Ashen Lighthouse':
            selected = game
    connection.execute('UPDATE ui_state SET selected_game=?', (selected,))
    connection.commit()
    connection.close()
    database.chmod(0o600)
    (root/'Game.exe').write_bytes(b'MZ')
    demo = root/'ashen-lighthouse'
    demo.write_text('#!/usr/bin/python3\nimport sys,time\n'
                    'lines=["Ashen Lighthouse 1.4.2 (linux-x86_64)","Vulkan: using device 0 (integrated GPU)",'
                    '"Audio: PipeWire, 48000 Hz stereo","Loading save slot 2 \\u2014 The Keeper\'s Watch",'
                    '"Shader cache: 1,284 entries","World streamed in 2.31 s","Ready."]\n'
                    'for line in lines:\n    print(line, flush=True); time.sleep(0.15)\n'
                    'print("warning: controller 1 not found, using keyboard", file=sys.stderr, flush=True)\n'
                    'time.sleep(0.3)\n')
    demo.chmod(0o755)
    (root/'prefix').mkdir()
    raise SystemExit(0)

import json
import shutil
import time
from native_support import Atspi, report, wait, find, visible, press, qt, check, scalar, nodes, runner_paths

out = root/'preview'
out.mkdir(exist_ok=True)


def shot(name, settle=.6):
    time.sleep(settle)
    qt('screenshot')
    shutil.copyfile(root/'screenshot.png', out/f'{name}.png')


try:
    wait(lambda: find('Search games', 'text'), timeout=30)
    qt('resize', width=1440, height=900)
    wait(lambda: visible('Ashen Lighthouse', 'list item'), timeout=30)
    shot('01-library-grid', 1.2)
    press('Artwork wall view'); shot('02-library-wall')
    press('List view'); shot('03-library-list')
    press('Grid view')
    press('Ashen Lighthouse', 'list item')
    wait(lambda: visible('Configure'))
    shot('04-detail-overview', 1.2)
    press('Configuration', 'page tab'); shot('05-detail-configuration')
    press('Files', 'page tab'); shot('06-detail-files')
    press('Overview', 'page tab')
    press('Play')
    wait(lambda: scalar('SELECT count(*) FROM launch_sessions') > 0, timeout=30)
    time.sleep(3)  # the demo prints for about 1.5 s, then exits
    press('Logs', 'page tab'); shot('07-detail-logs', 1.2)
    press('Overview', 'page tab')
    press('Configure'); wait(lambda: visible('Game name', 'text'))
    shot('08-configuration')
    press('Cancel'); wait(lambda: visible('Configure'))
    node = wait(lambda: find('Remove from library', 'button')); assert node.get_component_iface().grab_focus()
    press('Remove from library'); wait(lambda: visible('Remove entry'))
    shot('09-remove-confirm')
    press('Cancel')
    press('Settings'); wait(lambda: visible('Opaque panels', 'check box'))
    shot('10-settings')
    press('Runner settings');runner_paths(); wait(lambda: visible('UMU executable', 'text'))
    shot('11-runners')
    press('Cancel')
    press('Sessions and logs'); wait(lambda: visible('Captured session output', 'text'))
    shot('12-sessions')
    qt('key', sequence='Ctrl+N'); wait(lambda: visible('Game name', 'text'))
    shot('13-add-game')
    report['passed'] = True
except Exception as error:
    report['passed'] = False
    report['error'] = str(error)
    report['tree'] = []
    for node in nodes():
        try:
            states = node.get_state_set()
            report['tree'].append({'name': node.get_name(), 'role': node.get_role_name(),
                                   'showing': states.contains(Atspi.StateType.SHOWING),
                                   'actions': [node.get_action_iface().get_action_name(i) for i in range(node.get_action_iface().get_n_actions())] if node.get_action_iface() else []})
        except Exception:
            pass
    qt('screenshot'); shutil.copyfile(root/'screenshot.png', out/'failure.png')
    raise
finally:
    (root/'report.json').write_text(json.dumps(report, indent=2))
