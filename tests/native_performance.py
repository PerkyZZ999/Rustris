#!/usr/bin/python3
"""Observe a copied default desktop against a private 1,000-game library."""
import os
from pathlib import Path
import sqlite3
import sys
import time
import uuid

root = Path(sys.argv[1])
if len(sys.argv) == 3 and sys.argv[2] == '--prepare':
    import subprocess
    subprocess.run([sys.executable, str(Path(__file__).with_name('native_release_artwork.py')),
                    str(root), '--prepare'], check=True)
    database = root/'data/rustris/library.sqlite3'
    image = root/'home/art.png'
    with sqlite3.connect(database) as connection:
        for table in ('local_artwork', 'profiles', 'installations', 'games'):
            connection.execute(f'DELETE FROM {table}')
        for number in range(1000):
            game, installation, profile = (uuid.uuid4().bytes for _ in range(3))
            connection.execute('INSERT INTO games(id,display_name) VALUES(?,?)',
                               (game, f'Performance fixture {number:04}'))
            connection.execute('INSERT INTO installations VALUES(?,?)', (installation, game))
            connection.execute("INSERT INTO profiles(id,installation_id,executable,cwd,runner) VALUES(?,?,?,?,?)",
                               (profile, installation, b'/usr/bin/true', b'/', 'native'))
            connection.execute('INSERT INTO local_artwork VALUES(?,?)', (game, os.fsencode(image)))
        connection.execute('UPDATE ui_state SET selected_game=?', (game,))
    raise SystemExit(0)

if len(sys.argv) == 4 and sys.argv[2] == '--evict':
    # Drop clean cached pages of the copied pair, the private library and image,
    # and the main Qt libraries. Pages that another process maps (for example
    # Qt libraries used by the host desktop) stay resident; fincore records it.
    import json
    import subprocess
    application = Path(sys.argv[3]).resolve()
    files = [application, application.with_name('rustris-session'), root/'home/art.png',
             *sorted((root/'data/rustris').glob('library.sqlite3*'))]
    files += [Path(f'/usr/lib/libQt6{name}.so.6').resolve() for name in ('Core', 'Gui', 'Qml', 'Quick', 'QuickControls2')]
    def residency():
        # The kernel reports page-cache residency only for files the caller
        # could write, so system Qt libraries are recorded as unobservable.
        observed = {}
        for path in files:
            result = subprocess.run(['fincore', '-J', '-b', str(path)], capture_output=True, text=True)
            rows = json.loads(result.stdout or '{"fincore": []}')['fincore'] if result.returncode == 0 else []
            observed[str(path)] = {'resident': int(rows[0]['res']), 'size': int(rows[0]['size'])} if rows \
                else {'unobservable': result.stderr.strip()}
        return observed
    disabled = os.environ.get('RUSTRIS_PERF_NO_EVICT') == '1'
    before = residency()
    if not disabled:
        for path in files:
            descriptor = os.open(path, os.O_RDONLY | os.O_CLOEXEC)
            try:
                os.fsync(descriptor)  # freshly written fixture pages must be clean to drop
                os.posix_fadvise(descriptor, 0, 0, os.POSIX_FADV_DONTNEED)
            finally:
                os.close(descriptor)
    (root/'cache-residency.json').write_text(json.dumps({'evicted': not disabled, 'before': before, 'after': residency()}, indent=2)+'\n')
    raise SystemExit(0)

import json
import re
import subprocess
from native_support import Atspi, report, wait, find, visible, press, text, check, scalar, open_game, library, nodes
from eis_input import Input

started = float(sys.argv[4])
pid = int(sys.argv[5])
LOG = root/'application.log'


def since_start():
    return time.monotonic() - started


def selected():
    return scalar('SELECT display_name FROM games JOIN ui_state ON games.id=ui_state.selected_game')


def showing_rows():
    rows = []
    def walk(node, depth=0):
        if node is None or depth > 18:
            return
        try:
            name, role = node.get_name(), node.get_role_name()
            if role == 'list item' and name.startswith('Performance fixture ') \
                    and node.get_state_set().contains(Atspi.StateType.SHOWING):
                rows.append(int(name.rsplit(' ', 1)[1]))
            for index in range(node.get_child_count()):
                walk(node.get_child_at_index(index), depth+1)
        except Exception:
            return
    walk(Atspi.get_desktop(0))
    return sorted(rows)


def render_timing(begin, end):
    """Qt render-loop timing lines whose process time falls inside [begin, end] s."""
    rendered, prepared = [], []
    for line in LOG.read_text(errors='replace').splitlines():
        match = re.match(r'\s*(\d+\.\d+) qt\.scenegraph\.time\.renderloop: (.*)', line)
        if not match or not begin <= float(match[1]) <= end:
            continue
        values = {key: float(value) for key, value in re.findall(r'(\w+)=\s*([\d.]+)', match[2])}
        if 'polish' in values:
            prepared.append(values)
        elif 'rendered' in match[2] or 'render' in values:
            rendered.append(float(match[1]))
    gaps = sorted(round((b - a)*1000, 1) for a, b in zip(rendered, rendered[1:]))
    gui = sorted(round(sum(v.get(k, 0) for k in ('polish', 'lock', 'blockedForSync', 'animations')), 1) for v in prepared)
    def percentile(values, share):
        return values[min(len(values)-1, int(len(values)*share))] if values else None
    return {'frames': len(rendered), 'interval_ms_p50': percentile(gaps, .5), 'interval_ms_p95': percentile(gaps, .95),
            'interval_ms_max': gaps[-1] if gaps else None, 'intervals_over_50ms': sum(g > 50 for g in gaps),
            'gui_prepare_ms_p95': percentile(gui, .95), 'gui_prepare_ms_max': gui[-1] if gui else None,
            'gui_prepare_samples': len(gui)}


def wheel_scroll(device, label, steps=120, interval=.016):
    device.move(560, 520)
    before = showing_rows()
    begin = since_start()
    for _ in range(steps):
        device.wheel(-1)
        time.sleep(interval)
    time.sleep(.5)
    end = since_start()
    after = wait(lambda: (rows if (rows := showing_rows()) and rows != before else None), timeout=10)
    report['metrics'][f'wheel_{label}'] = {'steps': steps, 'interval_ms': interval*1000, 'rows_before': [before[0], before[-1]],
                                           'rows_after': [after[0], after[-1]], 'timing': render_timing(begin, end)}
    return before, after


try:
    wait(lambda: visible('Search games', 'text'), timeout=30)
    report['metrics']['startup_search_accessible_ms'] = round(since_start()*1000, 2)
    wait(lambda: visible('Performance fixture 0999', 'list item'), timeout=30)
    report['metrics']['startup_selected_row_accessible_ms'] = round(since_start()*1000, 2)
    # The artwork status caption lives on the game page; opening it adds one navigation.
    open_game('Performance fixture 0999')
    wait(lambda: visible('Local artwork selected.', 'label'), timeout=30)
    report['metrics']['startup_open_game_artwork_accessible_ms'] = round(since_start()*1000, 2)
    report['metrics']['cache'] = json.loads((root/'cache-residency.json').read_text())
    check('restored-selected-row-and-artwork-visible', True)
    library()
    press('Settings')
    wait(lambda: find('Opaque panels', 'check box'))
    report['metrics']['opaque_panels'] = find('Opaque panels', 'check box').get_state_set().contains(Atspi.StateType.CHECKED)
    report['metrics']['reduced_motion'] = find('Reduce motion', 'check box').get_state_set().contains(Atspi.StateType.CHECKED)
    press('Done')
    elapsed = []
    search_window = since_start()
    for _ in range(5):
        begin = time.monotonic(); text('Search games', '0999'); wait(lambda: find('Showing 1 of 1,000') or find('Showing 1 of 1000'))
        elapsed.append(round((time.monotonic()-begin)*1000, 2))
        text('Search games', ''); wait(lambda: find('Showing 1,000 of 1,000') or find('Showing 1000 of 1000'))
    report['metrics']['search_with_atspi_observation_ms'] = elapsed
    # App side: delegate creation happens in the GUI thread's polish step.
    report['metrics']['search_timing'] = render_timing(search_window, since_start())
    # Observer cost alone: one full accessibility-tree walk with nothing changing.
    walks = []
    for _ in range(5):
        begin = time.monotonic(); count = len(nodes()); walks.append(round((time.monotonic()-begin)*1000, 2))
    report['metrics']['atspi_tree_walk_ms'] = walks
    report['metrics']['atspi_tree_nodes'] = count
    device = Input()
    report['metrics']['input_driver'] = 'KWin EIS (libei) in the private compositor'
    item = wait(lambda: find('Performance fixture 0999', 'list item'))
    assert item.get_component_iface().grab_focus()
    begin = time.monotonic(); window = since_start()
    for _ in range(20):
        device.key('left'); time.sleep(.03)  # one entry back in the grid; about 50 ms per key
    last = time.monotonic()
    wait(lambda: selected() == 'Performance fixture 0979', timeout=30)
    persisted = time.monotonic()
    report['metrics']['twenty_up_paced_input_ms'] = round((last-begin)*1000, 2)
    report['metrics']['last_up_until_selection_persisted_ms'] = round((persisted-last)*1000, 2)
    try:
        wait(lambda: visible('Performance fixture 0979', 'list item'), timeout=10)
    except AssertionError:
        report['metrics']['showing_rows_after_keys'] = showing_rows()
        items = []
        def collect(node, depth=0):
            if node is None or depth > 18:
                return
            try:
                if node.get_role_name() == 'list item' and node.get_name().startswith('Performance fixture '):
                    states = node.get_state_set()
                    extents = node.get_component_iface().get_extents(Atspi.CoordType.WINDOW)
                    items.append([node.get_name()[-4:], states.contains(Atspi.StateType.SHOWING),
                                  states.contains(Atspi.StateType.VISIBLE), states.contains(Atspi.StateType.SELECTED),
                                  states.contains(Atspi.StateType.FOCUSED), extents.y, extents.height])
                for index in range(node.get_child_count()):
                    collect(node.get_child_at_index(index), depth+1)
            except Exception:
                return
        collect(Atspi.get_desktop(0))
        report['metrics']['accessible_rows_after_keys'] = items
        probe = {}
        library = find('Game library', 'list')
        probe['list_found'] = library is not None
        if library is not None:
            try:
                probe['child_count'] = library.get_child_count()
                probe['children'] = []
                for index in range(min(library.get_child_count(), 40)):
                    try:
                        child = library.get_child_at_index(index)
                        probe['children'].append([index, child.get_role_name(), child.get_name()[:40],
                                                  child.get_state_set().contains(Atspi.StateType.SHOWING)] if child else [index, None])
                    except Exception as error:
                        probe['children'].append([index, 'error', str(error)[:120]])
            except Exception as error:
                probe['error'] = str(error)[:200]
        report['metrics']['list_probe'] = probe
        subprocess.run(['spectacle', '--background', '--fullscreen', '--nonotify', '--output', str(root/'after-keys.png')],
                       capture_output=True, timeout=30)
        raise
    check('compositor-keyboard-selection', True)
    report['metrics']['last_up_until_row_showing_ms'] = round((time.monotonic()-last)*1000, 2)
    report['metrics']['keyboard_timing'] = render_timing(window, since_start())
    before, after = wheel_scroll(device, 'default')
    check('compositor-wheel-scrolls-list', after[-1] < before[0])
    press('Settings'); wait(lambda: find('Opaque panels', 'check box'))
    for label in ('Opaque panels', 'Reduce motion'):
        press(label, 'check box')
        wait(lambda: find(label, 'check box').get_state_set().contains(Atspi.StateType.CHECKED))
    press('Done'); wait(lambda: not visible('Opaque panels', 'check box'))
    before, after = wheel_scroll(device, 'opaque_reduced')
    check('compositor-wheel-scrolls-list-opaque-reduced', after[-1] < before[0])
    begin = time.monotonic(); press('Settings'); wait(lambda: find('Opaque panels', 'check box'))
    report['metrics']['settings_with_atspi_observation_ms'] = round((time.monotonic()-begin)*1000, 2)
    press('Done')
    status = Path(f'/proc/{pid}/status').read_text()
    report['metrics']['rss_kib'] = int(next(line.split()[1] for line in status.splitlines() if line.startswith('VmRSS:')))
    check('settings-remains-responsive', True)
    report['passed'] = True
except Exception as error:
    report['passed'] = False
    report['error'] = str(error)
    raise
finally:
    (root/'performance-report.json').write_text(json.dumps(report, indent=2)+'\n')
