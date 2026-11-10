#!/usr/bin/python3
"""One offline desktop run across native and both prepared UMU fixture pairs."""
from pathlib import Path
import os
from native_support import open_game, library, ROOT, Atspi, report, wait, find, visible, press, text, qt, check, db, scalar, nodes, json, sys, checked, runner_paths

sniper, ge = map(Path, sys.argv[4:6])


def enabled(name):
    node = visible(name, 'button') or visible(name, 'push button')
    return node is not None and node.get_state_set().contains(Atspi.StateType.ENABLED)


def state():
    with db() as connection:
        row = connection.execute('SELECT state FROM launch_sessions ORDER BY created_ms DESC LIMIT 1').fetchone()
        return row[0] if row else None


def output():
    node = find('Captured session output', 'text')
    return Atspi.Text.get_text(node, 0, -1) if node else ''


def add_profile(name, executable, arguments, prefix=None):
    press('Add game'); wait(lambda: find('Game name', 'text'))
    text('Game name', name); text('Executable path', str(executable)); text('Working directory', str(ROOT))
    if prefix is not None:
        press('UMU / Proton', 'radio button')
        text('Wine prefix', str(prefix))
    for index, argument in enumerate(arguments):
        press('Add argument'); text(f'Argument {index+1}', argument)
    press('Save profile')
    wait(lambda: enabled('Review and play' if prefix is not None else 'Play'))


def select_runner(inputs, runtime):
    press('Settings'); press('Runner settings');runner_paths()
    for name, value in [('UMU executable', '/usr/bin/umu-run'),
                        ('Proton directory', str(inputs/'proton')),
                        ('Prepared runtime directory', str(inputs/'data/umu'/runtime))]:
        text(name, value)
    press('Save runner settings'); wait(lambda: not visible('Save runner settings', 'button'))
    library()


def run_windows(inputs, runtime, label, expected_version, expected_runtime):
    prefix = ROOT/f'prefix-{label}'; prefix.mkdir(mode=0o700)
    result = ROOT/f'windows-{label}.json'
    select_runner(inputs, runtime)
    args = ['Z:'+str(result).replace('/', '\\'), 'hold', '8000', 'spaces and 🦀', '$(touch never)']
    add_profile('Combined '+label, inputs/'windows-fixture.exe', args, prefix)
    before = scalar('SELECT count(*) FROM launch_sessions')
    press('Review and play'); wait(lambda: visible('Launch game', 'button'), timeout=30)
    check(label+'-review-has-version-and-evidence', any(expected_version in n.get_name() for n in nodes()) and any('Evidence:' in n.get_name() for n in nodes()))
    check(label+'-review-does-not-spawn', scalar('SELECT count(*) FROM launch_sessions') == before and not (prefix/'drive_c').exists())
    press('Launch game'); wait(lambda: result.exists(), timeout=60)
    payload = json.loads(result.read_text())
    check(label+'-literal-arguments', payload['argv'][4:] == ['spaces and 🦀', '$(touch never)'] and not (ROOT/'never').exists())
    check(label+'-runner-log-visible', wait(lambda: 'umu-launcher version' in output()))
    check(label+'-honest-output-disclosure', wait(lambda: any('Windows application console output is unverified' in n.get_name() for n in nodes())))
    wait(lambda: enabled('Stop')); press('Stop'); wait(lambda: state() == 'stopped', timeout=30)
    check(label+'-owned-stop', scalar('SELECT count(*) FROM session_reservations') == 0)
    with db() as connection:
        saved = connection.execute('SELECT r.proton_version,r.runtime_version,r.prefix_path FROM session_runners r JOIN launch_sessions s ON s.id=r.session_id ORDER BY s.created_ms DESC LIMIT 1').fetchone()
    check(label+'-stored-runner-and-prefix', saved is not None and expected_version in saved[0] and saved[1] == expected_runtime and saved[2] == os.fsencode(prefix))
    library(); wait(lambda: not visible('Captured session output', 'text'))


try:
    wait(lambda: find('Search games', 'text'))
    press('Settings'); press('Opaque panels', 'check box')
    check('settings-opaque-enabled', wait(lambda: checked('Opaque panels')))
    press('Done'); wait(lambda: not visible('Opaque panels', 'check box'))
    native = ROOT/'native-fixture.py'
    native.write_text("import time\nprint('combined native output',flush=True)\ntime.sleep(20)\n")
    add_profile('Combined native', '/usr/bin/python3', [str(native)])
    press('Configure'); wait(lambda: find('Game name', 'text'))
    text('Game name', 'Combined native edited'); press('Save profile')
    check('native-profile-edit-persisted', wait(lambda: scalar('SELECT revision FROM profiles') == 2))
    text('Search games', 'native edited')
    check('library-search-finds-edited-profile', wait(lambda: find('Combined native edited', 'list item')))
    text('Search games', '')
    open_game('Combined native edited')
    wait(lambda: enabled('Play'))
    press('Play'); wait(lambda: state() == 'running'); wait(lambda: enabled('Stop'))
    check('native-log-visible', wait(lambda: 'combined native output' in output()))
    press('Preflight evidence')
    check('native-preflight-evidence', wait(lambda: any('executable: available' in n.get_name() and 'Evidence:' in n.get_name() for n in nodes())))
    press('Back to session'); press('Stop'); wait(lambda: state() == 'stopped')
    check('native-owned-stop', scalar('SELECT count(*) FROM session_reservations') == 0)
    library(); wait(lambda: not visible('Captured session output', 'text'))
    run_windows(sniper, 'steamrt3', 'Sniper', 'UMU-Proton-10.0-4', '3.0.20260805.254768')
    run_windows(ge, 'steamrt4', 'GE', 'GE-Proton11-5', '4.0.20261021.265997')
    check('combined-library-and-sessions-persist', scalar('SELECT count(*) FROM games') == 3 and scalar('SELECT count(*) FROM launch_sessions') == 3)
    press('Settings'); check('settings-persist-in-run', wait(lambda: checked('Opaque panels'))); press('Done')
    report['passed'] = True
except Exception as error:
    report['passed'] = False; report['error'] = str(error); report['tree'] = []
    for node in nodes():
        try: report['tree'].append({'name': node.get_name(), 'role': node.get_role_name()})
        except Exception: pass
    raise
finally:
    (ROOT/'report.json').write_text(json.dumps(report, indent=2))
