#!/usr/bin/python3
"""Bounded synthetic process fixture; writes only its explicit private report root."""
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time

report = Path(os.environ['FIXTURE_REPORT'])
mode = os.environ.get('FIXTURE_MODE', 'echo')
if mode == 'ignore_term':
    signal.signal(signal.SIGTERM, signal.SIG_IGN)
# Publish the report whole: tests poll for the name and read it immediately.
partial = report.with_name(report.name + '.partial')
with partial.open('w') as stream:
    json.dump({'argv': [os.fsencode(arg).hex() for arg in sys.argv[1:]],
               'cwd': os.getcwdb().hex(), 'pid': os.getpid(),
               'raw': os.environb.get(b'KEY_\xff', b'').hex(),
               'ambient_secret_present': 'FIXTURE_AMBIENT_TOKEN' in os.environ}, stream)
os.replace(partial, report)
if mode == 'wrapper':
    environment = os.environ.copy()
    environment.update(FIXTURE_MODE='sleep', FIXTURE_REPORT=str(report.with_name('child-report.json')))
    subprocess.Popen([sys.executable, __file__], env=environment,
                     start_new_session=os.environ.get('FIXTURE_CHILD_SETSID') == '1')
    sys.exit(0)
if mode == 'flood':
    chunk = b'X' * (32 * 1024)
    for _ in range(1024):
        os.write(1, chunk)
        os.write(2, chunk)
elif mode == 'stream':
    chunk = b'X' * (32 * 1024)
    for _ in range(2048):
        os.write(1, chunk)
        time.sleep(0.002)
    report.with_name('stream-complete').write_text('finished')
else:
    print('fixture output', flush=True)
duration = min(15.0, max(0.0, float(os.environ.get('FIXTURE_SECONDS', '0.2'))))
end = time.monotonic() + duration
counter = 0
while time.monotonic() < end:
    report.with_name('heartbeat').write_text(str(counter))
    counter += 1
    time.sleep(0.02)
sys.exit(int(os.environ.get('FIXTURE_EXIT', '7')))
