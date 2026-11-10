#!/usr/bin/python3
"""Rapid artwork selection in a copied default build and private compositor."""
import os
from pathlib import Path
import sqlite3
import subprocess
import sys
import uuid

root = Path(sys.argv[1])
if len(sys.argv) == 3 and sys.argv[2] == '--prepare':
    import struct
    import zlib

    def chunk(kind, data):
        return struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data))

    def solid(color):
        side = 3000
        row = b'\0' + bytes(color) * side
        compressor = zlib.compressobj(6)
        compressed = b''.join(compressor.compress(row) for _ in range(side)) + compressor.flush()
        return (b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', side, side, 8, 2, 0, 0, 0))
                + chunk(b'IDAT', compressed) + chunk(b'IEND', b''))

    subprocess.run([sys.executable, str(Path(__file__).with_name('native_release_artwork.py')),
                    str(root), '--prepare'], check=True)
    directory = root/'home/race'
    directory.mkdir()
    red, blue = solid((230, 30, 25)), solid((20, 60, 230))
    with sqlite3.connect(root/'data/rustris/library.sqlite3') as connection:
        for table in ('local_artwork', 'profiles', 'installations', 'games'):
            connection.execute(f'DELETE FROM {table}')
        for number in range(6):
            image = directory/f'{number:04}.png'
            image.write_bytes(red if number == 0 else blue)
            game, installation, profile = (uuid.uuid4().bytes for _ in range(3))
            connection.execute('INSERT INTO games(id,display_name) VALUES(?,?)',
                               (game, f'Release race {number:04}'))
            connection.execute('INSERT INTO installations VALUES(?,?)', (installation, game))
            connection.execute("INSERT INTO profiles(id,installation_id,executable,cwd,runner) VALUES(?,?,?,?,?)",
                               (profile, installation, b'/usr/bin/true', b'/', 'native'))
            connection.execute('INSERT INTO local_artwork VALUES(?,?)', (game, os.fsencode(image)))
        connection.execute('UPDATE ui_state SET selected_game=?', (game,))
    raise SystemExit(0)

import json
import time
from PIL import Image
from native_support import library, Atspi, ROOT, report, wait, find, visible, press, check, scalar


def selected():
    return scalar('SELECT display_name FROM games JOIN ui_state ON games.id=ui_state.selected_game')


def screenshot_colors(name):
    choice = wait(lambda: find('Choose artwork…', 'button'))
    assert choice.get_component_iface().grab_focus()
    wait(lambda: visible('Local artwork selected.', 'label'), timeout=30)
    path = ROOT/f'{name}.png'
    result = subprocess.run(['spectacle', '--background', '--fullscreen', '--nonotify', '--output', str(path)],
                            capture_output=True, text=True, timeout=30)
    assert result.returncode == 0 and path.exists(), result.stderr
    with Image.open(path) as image:
        panel = image.convert('RGB').crop((round(image.width * .75), 0, image.width, image.height))
        pixels = panel.getcolors(panel.width * panel.height)
        counts = dict((color, count) for count, color in pixels)
        return {'red': counts.get((230, 30, 25), 0), 'blue': counts.get((20, 60, 230), 0)}

try:
    wait(lambda: visible('Release race 0005', 'list item'), timeout=30)
    press('Release race 0005', 'list item')
    wait(lambda: visible('Local artwork selected.', 'label'), timeout=30)
    initial = screenshot_colors('release-race-initial')
    report['metrics']['initial_pixels'] = initial
    check('initial-blue-image-pixels', initial['blue'] > 100 and initial['red'] == 0)
    began = time.monotonic()
    states = []
    for number in (4, 3, 2, 1, 5, 4, 3, 2, 1, 0):
        name = f'Release race {number:04}'
        library()  # selecting a game opens its page; return for the next one
        press(name, 'list item')
        wait(lambda: (item := find(name, 'list item')) and item.get_state_set().contains(Atspi.StateType.SELECTED))
        unavailable = visible('Artwork unavailable or unreadable. The original selection is preserved.', 'label') is not None
        states.append({'selection': name, 'unavailable_label': unavailable})
    report['metrics']['selection_states'] = states
    check('all-selection-transitions-observed', len(states) == 10)
    wait(lambda: selected() == 'Release race 0000', timeout=30)
    wait(lambda: visible('Local artwork selected.', 'label'), timeout=30)
    final = screenshot_colors('release-race-final')
    report['metrics']['final_pixels'] = final
    check('final-red-image-pixels', final['red'] > 100 and final['blue'] == 0)
    time.sleep(1)
    later = screenshot_colors('release-race-later')
    report['metrics']['later_pixels'] = later
    check('final-image-stays-red', selected() == 'Release race 0000' and later['red'] > 100 and later['blue'] == 0)
    check('desktop-remains-responsive', visible('Settings', 'button') is not None)
    check('no-test-transport', not (ROOT/'qt-response.json').exists())
    report['metrics']['elapsed_ms_including_screenshots'] = round((time.monotonic()-began)*1000, 2)
    report['passed'] = True
except Exception as error:
    report['passed'] = False
    report['error'] = str(error)
    raise
finally:
    (ROOT/'release-artwork-race-report.json').write_text(json.dumps(report, indent=2)+'\n')
