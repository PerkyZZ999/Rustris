#!/usr/bin/python3
"""Check WCAG 2.x contrast for the colour pairs the QML actually uses.

Reads apps/desktop/qml/components/Theme.qml, so a token change is re-checked by
tests/check.sh. Text needs 4.5:1 (1.4.3) and input boundaries, focus rings and
control indicators 3:1 (1.4.11). Translucent surfaces (#AARRGGBB, as Qt writes
them) are composited over the window background first, and the frosted glass is
checked at both ends of its gradient. Disabled controls are exempt from 1.4.3.
"""
from pathlib import Path
import re
import sys

THEME = Path(__file__).resolve().parents[1]/'apps/desktop/qml/components/Theme.qml'
tokens = dict(re.findall(r'property color (\w+): "(#[0-9a-fA-F]{6}(?:[0-9a-fA-F]{2})?)"', THEME.read_text()))


def rgba(value):
    if not value.startswith('#') and value not in tokens:
        sys.exit(f'FAIL Theme.qml has no colour token {value!r}')
    value = tokens.get(value, value).lstrip('#')
    alpha = int(value[:2], 16)/255 if len(value) == 8 else 1
    value = value[-6:]
    return tuple(int(value[i:i+2], 16) for i in (0, 2, 4)), alpha


def over(top, bottom='background'):
    (colour, alpha), (base, _) = rgba(top), rgba(bottom)
    return '#' + ''.join(f'{round(a*alpha + b*(1-alpha)):02x}' for a, b in zip(colour, base))


def luminance(value):
    channels = [c/255 for c in rgba(value)[0]]
    linear = [c/12.92 if c <= 0.03928 else ((c+0.055)/1.055)**2.4 for c in channels]
    return .2126*linear[0] + .7152*linear[1] + .0722*linear[2]


def ratio(a, b):
    high, low = sorted((luminance(a), luminance(b)), reverse=True)
    return (high+.05)/(low+.05)


# Surfaces that text or inputs actually sit on (surface.elevated is not one).
panels = {'base': 'background', 'raised': 'raisedBase', 'graphite': 'graphite',
          'pane': over('pane'), 'sidebar': over('sidebar'), 'header': over('header'),
          'glass top': over('glassTop'), 'glass bottom': over('glassBottom'), 'overlay': over('overlay'),
          'selected row': over('#14ff681f', over('pane'))}
pairs = []
for ink in ('ink', 'secondary', 'muted', 'soft', 'danger'):
    for name, panel in panels.items():
        pairs.append((f'{ink} on {name}', ink, panel, 4.5))
pairs += [('on-accent text on accent top', 'accentText', 'accentTop', 4.5),
          ('on-accent text on accent bottom', 'accentText', 'accentStrong', 4.5),
          ('on-accent text on hover top', 'accentText', 'accentHoverTop', 4.5),
          ('active chip text', 'accentChipText', over('accentSoft', over('pane')), 4.5),
          ('success text on its pill', 'successText', over('#243dd68c', over('pane')), 4.5),
          ('warning on its pill', 'warning', over('#29f5c451', over('pane')), 4.5),
          ('accent link on pane', 'accentLink', over('pane'), 4.5)]
for name, panel in panels.items():
    pairs.append((f'input boundary on {name}', 'control', panel, 3.0))
    pairs.append((f'focus ring on {name}', 'ember', panel, 3.0))
pairs += [('switch on against pane', 'accent', over('pane'), 3.0),
          ('selected card ring on base', 'accent', 'background', 3.0)]

failed = False
for label, foreground, background, minimum in pairs:
    value = ratio(foreground, background)
    status = 'PASS' if value >= minimum else 'FAIL'
    failed |= value < minimum
    print(f'{status} {label}: {value:.2f}:1 (needs {minimum}:1)')
sys.exit(1 if failed else 0)
