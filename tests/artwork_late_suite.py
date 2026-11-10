#!/usr/bin/python3
"""Hold a decoded response while the selected game changes in a private desktop."""
from pathlib import Path
from PIL import Image
from native_support import open_game, library, ROOT, report, wait, find, visible, press, qt, check, scalar, json, time


def selected():
    return scalar('SELECT display_name FROM games JOIN ui_state ON games.id=ui_state.selected_game')


def artwork_pixel(state):
    qt('screenshot')
    with Image.open(ROOT/'screenshot.png') as image:
        x = round(state['artworkCenterX'] * state['dpr'])
        y = round(state['artworkCenterY'] * state['dpr'])
        assert 0 <= x < image.width and 0 <= y < image.height
        return image.convert('RGB').getpixel((x, y))

try:
    wait(lambda: visible('Release race 0005', 'list item'), timeout=30)
    wait(lambda: qt('state').get('artworkStatus') == 1, timeout=30)
    qt('arm-artwork-gate', path='home/race/0004.png')
    press('Release race 0004', 'list item')
    old = wait(lambda: (state if (state := qt('state')).get('artworkGateBlocked') else None), timeout=15)
    check('old-blue-response-decoded-and-held', old['artworkSource'].endswith('/0004.png') and old['artworkStatus'] == 2)
    library()
    press('Release race 0000', 'list item')
    pending = wait(lambda: (state if (state := qt('state')).get('artworkSource','').endswith('/0000.png')
                            and state.get('artworkStatus') == 2 else None), timeout=15)
    check('new-red-selection-waits-behind-held-response', pending['artworkGateBlocked'] and selected() == 'Release race 0000')
    qt('release-artwork-gate')
    completed = wait(lambda: (state if (state := qt('state')).get('artworkGateCompleted') else None), timeout=15)
    check('held-response-cancelled-before-release', completed['artworkGateCancelled'] and not completed['artworkGateTimedOut'])
    final = wait(lambda: (state if (state := qt('state')).get('artworkSource','').endswith('/0000.png')
                          and state.get('artworkStatus') == 1 else None), timeout=30)
    final = qt('state'); pixel = artwork_pixel(final)
    check('selected-red-image-rendered', pixel[0] > 180 and pixel[2] < 80)
    time.sleep(1)
    later = qt('state'); later_pixel = artwork_pixel(later)
    check('late-blue-response-does-not-replace-red', later['artworkSource'].endswith('/0000.png')
          and later['artworkStatus'] == 1 and later_pixel[0] > 180 and later_pixel[2] < 80)
    report['metrics'].update(old=old, pending=pending, completed=completed, final_pixel=pixel, later_pixel=later_pixel)
    report['passed'] = True
except Exception as error:
    report['passed'] = False
    report['error'] = str(error)
    raise
finally:
    try:
        if qt('state').get('artworkGateBlocked'):
            qt('release-artwork-gate')
    finally:
        (ROOT/'report.json').write_text(json.dumps(report, indent=2)+'\n')
