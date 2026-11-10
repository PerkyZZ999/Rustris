#!/usr/bin/env python3
"""Generic, private KWin/DBus test transport for the agent-cu accessibility CLI."""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time

PARSER=argparse.ArgumentParser()
PARSER.add_argument('--root',type=Path,required=True)
PARSER.add_argument('--scale',type=float,default=1)
PARSER.add_argument('--inner',action='store_true')
PARSER.add_argument('--application',type=Path)
PARSER.add_argument('--suite',action='store_true')
PARSER.add_argument('--smoke',action='store_true')
PARSER.add_argument('--sessions',action='store_true')
PARSER.add_argument('--keyboard',action='store_true')
PARSER.add_argument('--keyboard-size',choices=['1280x800','1440x900'],default='1280x800')
PARSER.add_argument('--native-title',type=Path)
PARSER.add_argument('--release-sessions',action='store_true')
PARSER.add_argument('--umu-inputs',type=Path)
PARSER.add_argument('--combined-inputs',nargs=2,type=Path,metavar=('SNIPER','GE'))
PARSER.add_argument('--windows-title',action='store_true')
PARSER.add_argument('--artwork',action='store_true')
PARSER.add_argument('--release-artwork',action='store_true')
PARSER.add_argument('--release-artwork-race',action='store_true')
PARSER.add_argument('--performance',action='store_true')
PARSER.add_argument('--artwork-stress',action='store_true')
PARSER.add_argument('--artwork-race',action='store_true')
PARSER.add_argument('--artwork-late',action='store_true')
PARSER.add_argument('--layout-audit',action='store_true')
PARSER.add_argument('--preview',action='store_true')
PARSER.add_argument('--ime',choices=['default','ibus'])
PARSER.add_argument('--artwork-swap',action='store_true')
PARSER.add_argument('--release-umu',action='store_true')
PARSER.add_argument('--artwork-limits',action='store_true')
PARSER.add_argument('--artwork-reject',action='store_true')
PARSER.add_argument('--nivalis-title',action='store_true')
PARSER.add_argument('--nivalis-preflight',action='store_true')
PARSER.add_argument('--nivalis-ge-preflight',action='store_true')
PARSER.add_argument('--ge11-probe',type=Path)
PARSER.add_argument('--ge11-nivalis',action='store_true')
PARSER.add_argument('--ge11-helper',type=Path)
ARGS=PARSER.parse_args()
if ARGS.ge11_nivalis and not ARGS.ge11_probe:raise SystemExit('Nivalis GE feasibility requires a private probe copy')
if ARGS.ge11_helper and not ARGS.ge11_probe:raise SystemExit('GE helper tests require a private probe copy')
if ARGS.ime:
    other_modes=('suite','smoke','sessions','keyboard','native_title','release_sessions','umu_inputs',
                 'combined_inputs','windows_title','artwork','release_artwork','release_artwork_race',
                 'performance','artwork_stress','artwork_race','artwork_late','layout_audit','artwork_swap',
                 'release_umu','artwork_limits','artwork_reject','nivalis_title','nivalis_preflight',
                 'nivalis_ge_preflight','ge11_probe')
    if not ARGS.application or any(getattr(ARGS,name) for name in other_modes):
        raise SystemExit('Composed-text fixture requires one explicit application and no other mode')
if ARGS.layout_audit:
    other_modes=('suite','smoke','sessions','keyboard','native_title','release_sessions','umu_inputs',
                 'combined_inputs','windows_title','artwork','release_artwork','release_artwork_race',
                 'performance','artwork_stress','artwork_race','artwork_late','artwork_swap','release_umu',
                 'artwork_limits','artwork_reject','nivalis_title','nivalis_preflight','nivalis_ge_preflight','ge11_probe')
    if not ARGS.application or any(getattr(ARGS,name) for name in other_modes):
        raise SystemExit('Layout audit requires one explicit test application and no other mode')
if ARGS.artwork_late:
    other_modes=('suite','smoke','sessions','keyboard','native_title','release_sessions','umu_inputs',
                 'combined_inputs','windows_title','artwork','release_artwork','release_artwork_race',
                 'performance','artwork_stress','artwork_race','artwork_swap','release_umu','artwork_limits',
                 'artwork_reject','nivalis_title','nivalis_preflight','nivalis_ge_preflight','ge11_probe')
    if not ARGS.application or any(getattr(ARGS,name) for name in other_modes):
        raise SystemExit('Late artwork fixture requires one explicit test application and no other mode')
if ARGS.release_artwork_race:
    other_modes=('suite','smoke','sessions','keyboard','native_title','release_sessions','umu_inputs',
                 'combined_inputs','windows_title','artwork','release_artwork','performance','artwork_stress',
                 'artwork_race','artwork_late','layout_audit','artwork_swap','release_umu','artwork_limits','artwork_reject',
                 'nivalis_title','nivalis_preflight','nivalis_ge_preflight','ge11_probe')
    if not ARGS.application or any(getattr(ARGS,name) for name in other_modes):
        raise SystemExit('Release artwork race requires one explicit application and no other mode')
if ARGS.performance:
    other_modes=('suite','smoke','sessions','keyboard','native_title','release_sessions','umu_inputs',
                 'combined_inputs','windows_title','artwork','release_artwork','release_artwork_race','artwork_stress',
                 'artwork_race','artwork_late','layout_audit','artwork_swap','release_umu','artwork_limits','artwork_reject',
                 'nivalis_title','nivalis_preflight','nivalis_ge_preflight','ge11_probe')
    if not ARGS.application or any(getattr(ARGS,name) for name in other_modes):
        raise SystemExit('Performance fixture requires one explicit application and no other mode')
if ARGS.combined_inputs:
    from umu_fixture_support import identified_runner_copy
    owned=(Path(__file__).resolve().parents[1]/'artifacts/s3/runtime').resolve()
    combined=[path.resolve() for path in ARGS.combined_inputs]
    if len(set(combined))!=2 or any(path.parent!=owned or not identified_runner_copy(path,owned) for path in combined):
        raise SystemExit('Combined test requires two distinct identified private runner copies')
    other_modes=('suite','smoke','sessions','keyboard','native_title','release_sessions','umu_inputs',
                 'windows_title','artwork','release_artwork','release_artwork_race','performance','artwork_stress','artwork_race',
                 'artwork_late','layout_audit','artwork_swap','release_umu','artwork_limits','artwork_reject','nivalis_title',
                 'nivalis_preflight','nivalis_ge_preflight','ge11_probe','ge11_nivalis','ge11_helper')
    if any(getattr(ARGS, name) for name in other_modes):
        raise SystemExit('Combined fixture cannot include another desktop test mode')
    if not ARGS.application:
        raise SystemExit('Combined fixture requires an explicit test-build application')
ROOT=ARGS.root.resolve()
CU=shutil.which('agent-cu')
if not CU:raise SystemExit('agent-cu is required for this test session')

if not ARGS.inner:
    ROOT.mkdir(parents=True,mode=0o700,exist_ok=False)
    for name in ('home','runtime','data','config','cache','state','tmp'):
        (ROOT/name).mkdir(mode=0o700)
    env=os.environ.copy()
    for name in ('DBUS_SESSION_BUS_ADDRESS','QML_IMPORT_PATH','QML2_IMPORT_PATH','QT_PLUGIN_PATH','QT_QPA_PLATFORM_PLUGIN_PATH','WAYLAND_DISPLAY','DISPLAY'):
        env.pop(name,None)
    env.update(HOME=str(ROOT/'home'),XDG_RUNTIME_DIR=str(ROOT/'runtime'),XDG_DATA_HOME=str(ROOT/'data'),
               XDG_CONFIG_HOME=str(ROOT/'config'),XDG_CACHE_HOME=str(ROOT/'cache'),XDG_STATE_HOME=str(ROOT/'state'),
               TMPDIR=str(ROOT/'tmp'),QT_LINUX_ACCESSIBILITY_ALWAYS_ON='1',QT_NO_XDG_DESKTOP_PORTAL='1',GTK_USE_PORTAL='0',
               XDG_SESSION_TYPE='wayland',QT_FORCE_STDERR_LOGGING='1')
    command=['bwrap','--die-with-parent','--unshare-net','--unshare-pid','--ro-bind','/','/',
             '--dev','/dev','--dev-bind','/dev/dri','/dev/dri','--proc','/proc','--tmpfs','/tmp',
             '--bind',str(ROOT),str(ROOT)]
    if ARGS.application and ARGS.application.resolve().is_relative_to('/tmp'):
        for binary in (ARGS.application.resolve(),ARGS.application.resolve().with_name('rustris-session')):
            if binary.exists():command.extend(['--ro-bind',str(binary),str(binary)])
    if ARGS.native_title:
        command.extend(['--bind',str(ARGS.native_title.resolve()),str(ARGS.native_title.resolve())])
    if ARGS.umu_inputs:
        command.extend(['--bind',str(ARGS.umu_inputs.resolve()),str(ARGS.umu_inputs.resolve())])
    if ARGS.combined_inputs:
        for path in combined:command.extend(['--bind',str(path),str(path)])
    if ARGS.ge11_probe:
        owned=Path(__file__).resolve().parents[1]/'artifacts/s3/runtime'
        if ARGS.ge11_probe.resolve().parent!=owned.resolve() or not (ARGS.ge11_probe/'probe-report.json').is_file():
            raise SystemExit('GE probe requires an identified private fixture copy')
        command.extend(['--bind',str(ARGS.ge11_probe.resolve()),str(ARGS.ge11_probe.resolve())])
    if ARGS.windows_title:
        from private_paths import private_path
        for target in (str(private_path('dragonwilds_game')),str(private_path('dragonwilds_prefix'))):
            command.extend(['--bind',target,target])
    command.extend(['--','dbus-run-session','--',sys.executable,
             str(Path(__file__).resolve()),'--root',str(ROOT),'--scale',str(ARGS.scale),'--inner'])
    if ARGS.application:command.extend(['--application',str(ARGS.application.resolve())])
    if ARGS.suite:command.append('--suite')
    if ARGS.smoke:command.append('--smoke')
    if ARGS.sessions:command.append('--sessions')
    if ARGS.keyboard:command.append('--keyboard')
    if ARGS.keyboard:command.extend(['--keyboard-size',ARGS.keyboard_size])
    if ARGS.release_sessions:command.append('--release-sessions')
    if ARGS.native_title:command.extend(['--native-title',str(ARGS.native_title.resolve())])
    if ARGS.umu_inputs:command.extend(['--umu-inputs',str(ARGS.umu_inputs.resolve())])
    if ARGS.combined_inputs:command.extend(['--combined-inputs',*(str(path) for path in combined)])
    if ARGS.windows_title:command.append('--windows-title')
    if ARGS.artwork:command.append('--artwork')
    if ARGS.release_artwork:command.append('--release-artwork')
    if ARGS.release_artwork_race:command.append('--release-artwork-race')
    if ARGS.performance:command.append('--performance')
    if ARGS.artwork_stress:command.append('--artwork-stress')
    if ARGS.artwork_race:command.append('--artwork-race')
    if ARGS.artwork_late:command.append('--artwork-late')
    if ARGS.layout_audit:command.append('--layout-audit')
    if ARGS.preview:command.append('--preview')
    if ARGS.ime:command.extend(['--ime',ARGS.ime])
    if ARGS.artwork_swap:command.append('--artwork-swap')
    if ARGS.release_umu:command.append('--release-umu')
    if ARGS.artwork_limits:command.append('--artwork-limits')
    if ARGS.artwork_reject:command.append('--artwork-reject')
    if ARGS.nivalis_title:command.append('--nivalis-title')
    if ARGS.nivalis_preflight:command.append('--nivalis-preflight')
    if ARGS.nivalis_ge_preflight:command.append('--nivalis-ge-preflight')
    if ARGS.ge11_probe:command.extend(['--ge11-probe',str(ARGS.ge11_probe.resolve())])
    if ARGS.ge11_nivalis:command.append('--ge11-nivalis')
    if ARGS.ge11_helper:command.extend(['--ge11-helper',str(ARGS.ge11_helper.resolve())])
    try:result=subprocess.run(command,env=env,check=False)
    except KeyboardInterrupt:raise SystemExit(130)
    raise SystemExit(result.returncode)

handles=[]
children=[]
def spawn(command,name,env=None,cwd=None):
    log=(ROOT/f'{name}.log').open('wb');handles.append(log)
    process=subprocess.Popen(command,env=env,cwd=cwd,stdout=log,stderr=subprocess.STDOUT);children.append(process);return process

def wait_for(predicate,timeout=20):
    deadline=time.monotonic()+timeout
    while time.monotonic()<deadline:
        value=predicate()
        if value:return value
        time.sleep(0.05)
    raise RuntimeError('Private desktop did not become ready')

try:
    compositor_env=os.environ.copy();compositor_env.update(QT_QPA_PLATFORM='offscreen',KWIN_SCREENSHOT_NO_PERMISSION_CHECKS='1')
    compositor_env.pop('KWIN_COMPOSE',None)
    # The performance fixture sends compositor-level input through KWin's EIS
    # interface on this private bus; other fixtures keep the default checks.
    if ARGS.performance or ARGS.ime:compositor_env['KWIN_WAYLAND_NO_PERMISSION_CHECKS']='1'
    if ARGS.ime:
        # A dead-key keymap for the private compositor only.
        (ROOT/'config'/'kxkbrc').write_text('[Layout]\nLayoutList=us\nVariantList=intl\nUse=true\n')
        compositor_env.update(XKB_DEFAULT_LAYOUT='us',XKB_DEFAULT_VARIANT='intl')
    socket='wayland-rustris-test'
    compositor=spawn(['kwin_wayland','--virtual','--no-lockscreen','--no-kactivities','--no-global-shortcuts',
                      '--width',str(round((1920 if ARGS.layout_audit else 1440)*ARGS.scale)),
                      '--height',str(round((1120 if ARGS.layout_audit else 1000)*ARGS.scale)),
                      '--socket',socket],'kwin',compositor_env)
    spawn(['/usr/lib/at-spi2-registryd'],'registry')
    wait_for(lambda:(ROOT/'runtime'/socket).exists())
    os.environ.update(WAYLAND_DISPLAY=socket,QT_QPA_PLATFORM='wayland',QML_DISABLE_DISK_CACHE='1',QSG_INFO='1')
    before=subprocess.run(['kscreen-doctor','-j'],capture_output=True,text=True,timeout=10,check=True)
    outputs=[output for output in json.loads(before.stdout)['outputs'] if output.get('enabled')]
    assert len(outputs)==1
    subprocess.run(['kscreen-doctor',f'output.{outputs[0]["id"]}.scale.{ARGS.scale}'],check=True,capture_output=True,timeout=10)
    output=subprocess.run(['kscreen-doctor','-j'],check=True,capture_output=True,text=True,timeout=10)
    (ROOT/'output.json').write_text(output.stdout)
    os.environ['RUSTRIS_TEST_DIRECTORY']=str(ROOT)
    if ARGS.release_artwork:
        subprocess.run([sys.executable,str(Path(__file__).with_name('native_release_artwork.py')),str(ROOT),'--prepare'],check=True)
    if ARGS.release_artwork_race:
        if not ARGS.application:raise SystemExit('Artwork race needs an explicit default application')
        subprocess.run([sys.executable,str(Path(__file__).with_name('native_release_artwork_race.py')),str(ROOT),'--prepare'],check=True)
    if ARGS.artwork_late:
        if not ARGS.application:raise SystemExit('Late artwork fixture needs an explicit test application')
        subprocess.run([sys.executable,str(Path(__file__).with_name('native_release_artwork_race.py')),str(ROOT),'--prepare'],check=True)
    if ARGS.performance:
        if not ARGS.application:raise SystemExit('Performance fixture requires an explicit default application')
        subprocess.run([sys.executable,str(Path(__file__).with_name('native_performance.py')),str(ROOT),'--prepare'],check=True)
        subprocess.run([sys.executable,str(Path(__file__).with_name('native_performance.py')),str(ROOT),'--evict',str(ARGS.application)],check=True)
        os.environ.update(QT_LOGGING_RULES='qt.scenegraph.time.renderloop=true',QT_MESSAGE_PATTERN='%{time process} %{category}: %{message}')
    if ARGS.preview:
        if not ARGS.application:raise SystemExit('Preview needs an explicit test application')
        subprocess.run([sys.executable,str(Path(__file__).with_name('ui_preview.py')),str(ROOT),'--prepare'],check=True)
    if ARGS.release_umu:
        if not ARGS.umu_inputs:raise SystemExit('Release UMU fixture needs private prepared inputs')
        subprocess.run([sys.executable,str(Path(__file__).with_name('native_release_umu.py')),str(ROOT),str(ARGS.umu_inputs),'--prepare'],check=True)
    if ARGS.ime=='ibus':
        spawn(['ibus-daemon','--replace','--panel=disable','--xim'],'ibus')
        def ibus_engine():
            # Selecting an xkb engine also runs X11 setxkbmap, which fails here
            # without an X display; the compositor already supplies the keymap,
            # so success is judged by querying the active engine.
            subprocess.run(['ibus','engine','xkb:us:intl:eng'],capture_output=True,text=True)
            result=subprocess.run(['ibus','engine'],capture_output=True,text=True)
            with (ROOT/'ibus-engine.log').open('a') as log:log.write(f'{result.returncode} {result.stdout.strip()} {result.stderr.strip()}\n')
            return result.stdout.strip()=='xkb:us:intl:eng'
        wait_for(ibus_engine)
        os.environ['QT_IM_MODULE']='ibus'
    elif ARGS.ime=='default':
        os.environ.pop('QT_IM_MODULE',None)
    if ARGS.application:
        application_started=time.monotonic()
        application=spawn([str(ARGS.application)],'application',cwd=ROOT/'home')
    if ARGS.ge11_probe:
        if ARGS.ge11_helper:
            binary=ARGS.ge11_helper.resolve()
            if not binary.is_relative_to((Path(__file__).resolve().parents[1]/'target').resolve()):raise SystemExit('Expected a local test binary')
            (ARGS.ge11_probe/'cases').mkdir(mode=0o700,exist_ok=True)
            env=os.environ.copy();env.update(RUSTRIS_UMU_FIXTURE_ROOT=str(ARGS.ge11_probe),RUSTRIS_UMU_FIXTURE_RUNTIME='steamrt4',F003_TOKEN='synthetic-never-inherit')
            result=subprocess.run([str(binary),'actual_umu_','--ignored','--test-threads=1','--nocapture'],env=env,check=False,timeout=240)
            raise SystemExit(result.returncode)
        name='game_probe.py' if ARGS.ge11_nivalis else 'display_probe.py'
        result=subprocess.run([sys.executable,str(Path(__file__).resolve().parents[1]/'experiments/f004'/name),str(ARGS.ge11_probe)],check=False)
        raise SystemExit(result.returncode)
    if ARGS.release_sessions:
        result=subprocess.run([sys.executable,str(Path(__file__).with_name('native_release_smoke.py')),str(ROOT),str(ARGS.application),str(ARGS.scale)],check=False)
        raise SystemExit(result.returncode)
    if ARGS.release_artwork:
        result=subprocess.run([sys.executable,str(Path(__file__).with_name('native_release_artwork.py')),str(ROOT)],check=False)
        raise SystemExit(result.returncode)
    if ARGS.release_artwork_race:
        result=subprocess.run([sys.executable,str(Path(__file__).with_name('native_release_artwork_race.py')),str(ROOT),str(ARGS.application),str(ARGS.scale)],check=False)
        raise SystemExit(result.returncode)
    if ARGS.performance:
        result=subprocess.run([sys.executable,str(Path(__file__).with_name('native_performance.py')),str(ROOT),str(ARGS.application),str(ARGS.scale),str(application_started),str(application.pid)],check=False)
        raise SystemExit(result.returncode)
    if ARGS.release_umu:
        result=subprocess.run([sys.executable,str(Path(__file__).with_name('native_release_umu.py')),str(ROOT),str(ARGS.umu_inputs)],check=False)
        raise SystemExit(result.returncode)
    if ARGS.smoke:
        result=subprocess.run([sys.executable,str(Path(__file__).with_name('native_smoke.py')),str(ROOT)],check=False)
        raise SystemExit(result.returncode)
    if ARGS.native_title:
        result=subprocess.run([sys.executable,str(Path(__file__).with_name('native_title_suite.py')),str(ROOT),str(ARGS.application),str(ARGS.scale),str(ARGS.native_title)],check=False)
        raise SystemExit(result.returncode)
    if ARGS.sessions:
        result=subprocess.run([sys.executable,str(Path(__file__).with_name('native_session_suite.py')),str(ROOT),str(ARGS.application),str(ARGS.scale)],check=False)
        raise SystemExit(result.returncode)
    if ARGS.keyboard:
        result=subprocess.run([sys.executable,str(Path(__file__).with_name('keyboard_suite.py')),str(ROOT),str(ARGS.application),str(ARGS.scale),ARGS.keyboard_size],check=False)
        raise SystemExit(result.returncode)
    if ARGS.windows_title:
        if not ARGS.umu_inputs:raise SystemExit('The authorized Windows smoke needs prepared private runner copies')
        result=subprocess.run([sys.executable,str(Path(__file__).with_name('windows_title_suite.py')),str(ROOT),str(ARGS.application),str(ARGS.scale),str(ARGS.umu_inputs)],check=False)
        raise SystemExit(result.returncode)
    if ARGS.nivalis_title:
        if not ARGS.umu_inputs:raise SystemExit('The Nivalis smoke needs prepared private runner copies')
        result=subprocess.run([sys.executable,str(Path(__file__).with_name('nivalis_title_suite.py')),str(ROOT),str(ARGS.application),str(ARGS.scale),str(ARGS.umu_inputs)],check=False)
        raise SystemExit(result.returncode)
    if ARGS.nivalis_preflight:
        if not ARGS.umu_inputs:raise SystemExit('The Nivalis preflight needs prepared private runner copies')
        result=subprocess.run([sys.executable,str(Path(__file__).with_name('nivalis_title_suite.py')),str(ROOT),str(ARGS.application),str(ARGS.scale),str(ARGS.umu_inputs),'--preflight'],check=False)
        raise SystemExit(result.returncode)
    if ARGS.nivalis_ge_preflight:
        if not ARGS.umu_inputs:raise SystemExit('The Nivalis preflight needs prepared private runner copies')
        result=subprocess.run([sys.executable,str(Path(__file__).with_name('nivalis_title_suite.py')),str(ROOT),str(ARGS.application),str(ARGS.scale),str(ARGS.umu_inputs),'--ge-preflight'],check=False)
        raise SystemExit(result.returncode)
    if ARGS.artwork:
        result=subprocess.run([sys.executable,str(Path(__file__).with_name('artwork_ui_suite.py')),str(ROOT),str(ARGS.application),str(ARGS.scale)],check=False)
        raise SystemExit(result.returncode)
    if ARGS.artwork_stress:
        result=subprocess.run([sys.executable,str(Path(__file__).with_name('artwork_stress_suite.py')),str(ROOT),str(ARGS.application),str(ARGS.scale)],check=False)
        raise SystemExit(result.returncode)
    if ARGS.artwork_race:
        result=subprocess.run([sys.executable,str(Path(__file__).with_name('artwork_race_suite.py')),str(ROOT),str(ARGS.application),str(ARGS.scale)],check=False)
        raise SystemExit(result.returncode)
    if ARGS.artwork_late:
        result=subprocess.run([sys.executable,str(Path(__file__).with_name('artwork_late_suite.py')),str(ROOT),str(ARGS.application),str(ARGS.scale)],check=False)
        raise SystemExit(result.returncode)
    if ARGS.ime:
        result=subprocess.run([sys.executable,str(Path(__file__).with_name('ime_suite.py')),str(ROOT),str(ARGS.application),str(ARGS.scale),ARGS.ime],check=False)
        raise SystemExit(result.returncode)
    if ARGS.preview:
        result=subprocess.run([sys.executable,str(Path(__file__).with_name('ui_preview.py')),str(ROOT),str(ARGS.application),str(ARGS.scale)],check=False)
        raise SystemExit(result.returncode)
    if ARGS.layout_audit:
        result=subprocess.run([sys.executable,str(Path(__file__).with_name('layout_audit_suite.py')),str(ROOT),str(ARGS.application),str(ARGS.scale)],check=False)
        raise SystemExit(result.returncode)
    if ARGS.artwork_swap:
        result=subprocess.run([sys.executable,str(Path(__file__).with_name('artwork_swap_suite.py')),str(ROOT),str(ARGS.application),str(ARGS.scale)],check=False)
        raise SystemExit(result.returncode)
    if ARGS.artwork_limits:
        result=subprocess.run([sys.executable,str(Path(__file__).with_name('artwork_limits_suite.py')),str(ROOT),str(ARGS.application),str(ARGS.scale)],check=False)
        raise SystemExit(result.returncode)
    if ARGS.artwork_reject:
        result=subprocess.run([sys.executable,str(Path(__file__).with_name('artwork_reject_suite.py')),str(ROOT),str(ARGS.application),str(ARGS.scale)],check=False)
        raise SystemExit(result.returncode)
    if ARGS.combined_inputs:
        result=subprocess.run([sys.executable,str(Path(__file__).with_name('combined_alpha_suite.py')),str(ROOT),str(ARGS.application),str(ARGS.scale),*(str(path) for path in combined)],check=False)
        raise SystemExit(result.returncode)
    if ARGS.umu_inputs:
        result=subprocess.run([sys.executable,str(Path(__file__).with_name('umu_ui_suite.py')),str(ROOT),str(ARGS.application),str(ARGS.scale),str(ARGS.umu_inputs)],check=False)
        raise SystemExit(result.returncode)
    if ARGS.suite:
        result=subprocess.run([sys.executable,str(Path(__file__).with_name('native_suite.py')),
                               str(ROOT),str(ARGS.application),str(ARGS.scale)],check=False)
        raise SystemExit(result.returncode)
    import gi
    gi.require_version('Atspi','2.0')
    from gi.repository import Atspi
    def accessible_tree(node, depth=0):
        if depth>12:return None
        try:
            return {'name':node.get_name(),'role':node.get_role_name(),
                    'children':[accessible_tree(node.get_child_at_index(i),depth+1) for i in range(node.get_child_count())]}
        except Exception as error:return {'error':str(error)}
    (ROOT/'ready').write_text('private offline desktop ready\n')
    print(f'READY {ROOT}',flush=True)
    last=None
    while not (ROOT/'stop').exists():
        if compositor.poll() is not None:raise RuntimeError('Private compositor exited')
        request=ROOT/'request.json'
        if request.exists():
            data=json.loads(request.read_text())
            if data['id']!=last:
                (ROOT/'accessibility.json').write_text(json.dumps(accessible_tree(Atspi.get_desktop(0))))
                last=data['id'];argv=data['argv']
                allowed={'open','snapshot','tree','find','click','type','key','scroll','scroll-to','focused','get-value','text','apps','windows','move-window','resize-window','screenshot','wait-for','check-permissions'}
                assert argv and argv[0] in allowed,'Only agent-cu primitives are accepted'
                try:
                    result=subprocess.run([CU,*argv,'--no-cdp'],capture_output=True,text=True,timeout=20,check=False)
                    answer={'id':last,'code':result.returncode,'stdout':result.stdout,'stderr':result.stderr}
                except subprocess.TimeoutExpired as error:
                    answer={'id':last,'code':124,'stdout':str(error.stdout or ''),'stderr':f'agent-cu command timed out: {error.stderr or ""}'}
                temporary=ROOT/'response.tmp';temporary.write_text(json.dumps(answer));temporary.rename(ROOT/'response.json')
        time.sleep(0.04)
finally:
    for process in reversed(children):
        if process.poll() is None:
            process.terminate()
            try:process.wait(timeout=5)
            except subprocess.TimeoutExpired:process.kill();process.wait(timeout=3)
    for handle in handles:handle.close()
