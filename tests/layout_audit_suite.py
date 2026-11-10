#!/usr/bin/python3
"""Audit text clipping on every reachable screen at the specified window sizes.

Runs only inside desktop_session.py's private offline desktop with the explicit
test build. The test-only driver walks the Qt Quick item tree; screenshots are
kept in the private root for inspection.
"""
from native_support import open_game, library, ROOT, BINARY, Atspi, children, logs, subprocess, report, wait, find, visible, press, text, qt, check, db, scalar, nodes, json, sys, time, runner_paths
import shutil

LONG_NAME = 'The Extraordinarily Long Chronicles of an Overflowing Title: Definitive Anniversary Edition'
MISSING = ROOT/'library'/('a-deliberately-long-installation-folder-name-' * 3)/'Binaries'/'Win64'/'missing-game-executable-with-a-long-name.x86_64'
# 980x660 is the window's minimum size (App.qml minimumWidth/minimumHeight).
SIZES = ((980, 660), (1280, 800), (1440, 900), (1920, 1080))
audits = []


def enabled(name):
    node = visible(name, 'button') or visible(name, 'push button')
    return node is not None and node.get_state_set().contains(Atspi.StateType.ENABLED)


def latest_state():
    with db() as connection:
        row = connection.execute('SELECT state FROM launch_sessions ORDER BY created_ms DESC LIMIT 1').fetchone()
        return row[0] if row else None


def add(name, executable, arguments=()):
    if not (visible('Add game', 'button') or visible('Add game', 'push button')): library()
    press('Add game'); wait(lambda: find('Game name', 'text'))
    text('Game name', name); text('Executable path', str(executable)); text('Working directory', str(ROOT))
    for index, argument in enumerate(arguments):
        press('Add argument'); text(f'Argument {index + 1}', argument)
    press('Save profile'); wait(lambda: enabled('Play'))


def press_detail(name):
    node = wait(lambda: find(name, 'button') or find(name, 'push button'))
    assert node.get_component_iface().grab_focus(), name
    press(name)


def select(name):
    open_game(name)
    wait(lambda: scalar('SELECT display_name FROM games JOIN ui_state ON games.id=ui_state.selected_game') == name)


def shown(name):
    return visible(name, 'button') or visible(name, 'push button')


def escape(until_gone):
    qt('key', sequence='Escape')
    wait(lambda: not visible(*until_gone))


INTERACTIVE = {'push button', 'button', 'toggle button', 'check box', 'radio button', 'text', 'entry',
               'password text', 'combo box', 'list item', 'menu item', 'slider', 'spin button', 'page tab'}


def accessibility():
    """Showing interactive nodes without an accessible name, and role counts."""
    unnamed, roles = [], {}
    def walk(node, depth=0):
        if node is None or depth > 22:
            return
        try:
            role, name = node.get_role_name(), node.get_name()
            if node.get_state_set().contains(Atspi.StateType.SHOWING):
                roles[role] = roles.get(role, 0) + 1
                if role in INTERACTIVE and not name.strip():
                    unnamed.append(role)
            for index in range(node.get_child_count()):
                walk(node.get_child_at_index(index), depth + 1)
        except Exception:
            return
    walk(Atspi.get_desktop(0))
    return unnamed, roles


def audit(screen, mode, size):
    time.sleep(.3)  # let popup enter transitions and layouts settle
    answer = qt('layout-audit')
    layout = answer['layout']
    name = f'{mode}-{size[0]}x{size[1]}-{screen}.png'
    qt('screenshot'); shutil.copyfile(ROOT/'screenshot.png', ROOT/'layout'/name)
    unnamed, roles = accessibility()
    audits.append({'screen': screen, 'mode': mode, 'size': list(size), 'texts': layout['texts'],
                   'issues': layout['issues'], 'elided': layout['elided'], 'screenshot': name,
                   'unnamed_controls': unnamed, 'accessible_roles': roles})
    assert layout['texts'] > 0, screen


def set_mode(opaque):
    press('Settings'); wait(lambda: visible('Opaque panels', 'check box'))
    for label in ('Opaque panels', 'Reduce motion'):
        node = find(label, 'check box')
        if node.get_state_set().contains(Atspi.StateType.CHECKED) != opaque:
            press(label, 'check box')
            wait(lambda: find(label, 'check box').get_state_set().contains(Atspi.StateType.CHECKED) == opaque)
    press('Done'); wait(lambda: not visible('Opaque panels', 'check box'))


try:
    (ROOT/'layout').mkdir()
    pictures = ROOT/'home'
    for index in range(30):
        (pictures/f'cover-art-with-a-rather-long-descriptive-file-name-{index:02}.png').write_bytes(b'not decoded here')
    wait(lambda: find('Search games', 'text'))
    check('layout-window-dpr', abs(qt('state')['dpr'] - float(sys.argv[3])) < .01)
    for size in SIZES:
        qt('resize', width=size[0], height=size[1])
        wait(lambda: (qt('state')['width'], qt('state')['height']) == size)
        audit('empty-library', 'glass', size)
        # The title bar must keep every window control inside the window at each size.
        for control in ('Minimize', 'Maximize', 'Close'):
            node = visible(control, 'button') or visible(control, 'push button')
            box = node.get_extents(Atspi.CoordType.WINDOW)
            check(f'title-bar-{control.lower()}-inside-{size[0]}x{size[1]}',
                  box.width > 0 and box.x >= 0 and box.x + box.width <= size[0])
    add(LONG_NAME, '/usr/bin/python3', ('-c', 'import signal,time\nsignal.signal(signal.SIGTERM,signal.SIG_IGN)\ntime.sleep(600)'))
    add('Missing executable fixture', MISSING)
    press('Play'); wait(lambda: latest_state() == 'launch_failed', timeout=30)
    library()
    select(LONG_NAME)
    press('Play'); wait(lambda: latest_state() == 'running', timeout=30)
    wait(lambda: enabled('Stop')); press('Stop')
    wait(lambda: latest_state() == 'stop_requested', timeout=30)
    library()
    # Placeholder runner components and a UMU profile reach the review with failing evidence.
    runner = ROOT/'placeholder-runner'
    for directory in ('proton', 'runtime'): (runner/directory).mkdir(parents=True)
    (runner/'umu-run').write_bytes(b''); (runner/'umu-run').chmod(0o755)
    (ROOT/'umu-prefix').mkdir(); (ROOT/'windows').mkdir(); (ROOT/'windows'/'Game.exe').write_bytes(b'MZ')
    press('Settings'); wait(lambda: visible('Opaque panels', 'check box'))
    press('Runner settings');runner_paths(); wait(lambda: visible('UMU executable', 'text'))
    text('UMU executable', str(runner/'umu-run')); text('Proton directory', str(runner/'proton'))
    text('Prepared runtime directory', str(runner/'runtime'))
    press('Save runner settings'); wait(lambda: not visible('UMU executable', 'text'), timeout=30)
    library()
    press('Add game'); wait(lambda: find('Game name', 'text'))
    text('Game name', 'UMU review fixture'); text('Executable path', str(ROOT/'windows'/'Game.exe'))
    text('Working directory', str(ROOT/'windows'))
    press('UMU / Proton', 'radio button'); text('Wine prefix', str(ROOT/'umu-prefix'))
    press('Save profile'); wait(lambda: enabled('Review and play'))
    for mode in ('glass', 'opaque-reduced'):
        set_mode(mode == 'opaque-reduced')
        for size in SIZES:
            qt('resize', width=size[0], height=size[1])
            wait(lambda: (qt('state')['width'], qt('state')['height']) == size)
            select(LONG_NAME)
            audit('detail-overview', mode, size)
            for tab in ('Configuration', 'Files', 'Logs'):
                press(tab, 'page tab'); audit('detail-' + tab.lower(), mode, size)
            press('Overview', 'page tab')
            library()
            audit('library', mode, size)
            press('Artwork wall view'); audit('library-wall', mode, size)
            press('List view'); audit('library-list', mode, size)
            press('Grid view')
            press('Add game'); wait(lambda: visible('Game name', 'text'))
            audit('editor-new', mode, size)
            escape(('Game name', 'text'))
            select('Missing executable fixture')
            press('Configure'); wait(lambda: visible('Game name', 'text'))
            press('UMU / Proton', 'radio button'); wait(lambda: find('Wine prefix', 'text'))
            audit('editor-umu', mode, size)
            press('Cancel'); wait(lambda: not visible('Game name', 'text'))
            press('Settings'); wait(lambda: visible('Opaque panels', 'check box'))
            audit('settings', mode, size)
            press('Runner settings');runner_paths(); wait(lambda: visible('UMU executable', 'text'))
            audit('runners', mode, size)
            press('Cancel'); wait(lambda: not visible('UMU executable', 'text'))
            select('Missing executable fixture')
            press('Remove from library'); wait(lambda: shown('Remove entry'))
            audit('remove-confirm', mode, size)
            press('Cancel'); wait(lambda: not shown('Remove entry'))
            press('Choose artwork…'); wait(lambda: find('cover-art-with-a-rather-long-descriptive-file-name-00.png'))
            audit('artwork-picker', mode, size)
            qt('key', sequence='Escape'); time.sleep(.3)
            press('Enter artwork path…'); wait(lambda: visible('Artwork path', 'text'))
            audit('artwork-path', mode, size)
            escape(('Artwork path', 'text'))
            press('Sessions and logs'); wait(lambda: visible('Captured session output', 'text'))
            audit('sessions', mode, size)
            press('Preflight evidence'); wait(lambda: shown('Back to session'))
            audit('preflight', mode, size)
            press('Back to session'); wait(lambda: visible('Captured session output', 'text'))
            press('Done'); wait(lambda: not visible('Captured session output', 'text'))
            select(LONG_NAME)
            press('Logs', 'page tab'); wait(lambda: enabled('Force stop…'))
            press('Force stop…'); wait(lambda: shown('Force stop'))
            audit('force-stop-confirm', mode, size)
            press('Cancel'); time.sleep(.3)
            press('Overview', 'page tab')
            select('UMU review fixture')
            press('Review and play'); wait(lambda: visible('Review UMU launch', 'label'), timeout=30)
            audit('umu-review', mode, size)
            qt('key', sequence='Escape'); wait(lambda: not visible('Review UMU launch', 'label'))
            library()
            press('Add game'); wait(lambda: visible('Game name', 'text'))
            text('Game name', 'Unsaved layout fixture')
            qt('close'); wait(lambda: shown('Discard and close'))
            audit('discard-confirm', mode, size)
            press('Keep editing'); time.sleep(.3)
            escape(('Game name', 'text'))
            qt('close'); wait(lambda: shown('Keep open'))
            audit('close-with-sessions', mode, size)
            press('Keep open'); time.sleep(.3)
    # The library error state: a newer schema is refused with a visible error.
    select(LONG_NAME)
    press('Logs', 'page tab'); wait(lambda: enabled('Force stop…'))
    press('Force stop…'); wait(lambda: shown('Force stop'))
    press('Force stop'); wait(lambda: latest_state() in ('stopped', 'exited'), timeout=30)
    library()
    set_mode(False)
    qt('close'); wait(lambda: find('Rustris', 'application') is None, timeout=30)
    with db() as connection: connection.execute('PRAGMA user_version=99')
    (ROOT/'qt-request.json').unlink(missing_ok=True)
    log = (ROOT/'layout-restart.log').open('w'); logs.append(log)
    children.append(subprocess.Popen([str(BINARY)], stdout=log, stderr=subprocess.STDOUT, cwd=ROOT/'home'))
    wait(lambda: shown('Reload library'), timeout=30)
    for size in SIZES:
        qt('resize', width=size[0], height=size[1])
        wait(lambda: (qt('state')['width'], qt('state')['height']) == size)
        audit('library-error', 'glass', size)
    issues = [dict(item, screen=a['screen'], mode=a['mode'], size=a['size']) for a in audits for item in a['issues']]
    report['metrics'].update(audits=len(audits), screens=sorted({a['screen'] for a in audits}),
                             issues=issues, elided=sum(len(a['elided']) for a in audits))
    (ROOT/'layout-audit.json').write_text(json.dumps(audits, indent=2) + '\n')
    check('every-screen-audited', len(audits) == len(SIZES) * (1 + 2 * 20 + 1))
    check('no-text-overflow-or-clipping', not issues)
    unnamed = [(a['screen'], a['mode'], a['size'], a['unnamed_controls']) for a in audits if a['unnamed_controls']]
    report['metrics']['unnamed_controls'] = unnamed
    check('every-showing-control-has-an-accessible-name', not unnamed)
    check('library-rows-exposed-as-list-items', all(a['accessible_roles'].get('list item', 0) >= 2
                                                    for a in audits if a['screen'] in ('library', 'library-wall', 'library-list')))
    report['passed'] = True
except Exception as error:
    report['passed'] = False; report['error'] = str(error); report['tree'] = []
    (ROOT/'layout-audit.json').write_text(json.dumps(audits, indent=2) + '\n')
    for node in nodes():
        try: report['tree'].append({'name': node.get_name(), 'role': node.get_role_name()})
        except Exception: pass
    raise
finally:
    (ROOT/'report.json').write_text(json.dumps(report, indent=2))
