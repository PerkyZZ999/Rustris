"""Paths to the owner's own games, prefixes and runners for opt-in real-title checks.

They are read from the git-ignored ``tests/private-paths.toml`` so personal
locations never reach the repository. Synthetic suites never call this; a
real-title run stops with a clear message when a key is missing.

Example ``tests/private-paths.toml``::

    minecraft = "/games/Minecraft_Linux/Minecraft_26.3"
    nivalis_game = "/games/Nivalis Nights"
    nivalis_prefix = "/home/me/My Games/Nivalis Nights"
    dragonwilds_game = "/games/RuneScape Dragonwilds"
    dragonwilds_prefix = "/home/me/My Games/RunescapeDragonwilds"
    umu_proton = "/home/me/.local/share/Steam/compatibilitytools.d/UMU-Proton-10.0-4"
    umu_runtime = "/home/me/.local/share/umu/steamrt3"
"""
import tomllib
from pathlib import Path

FILE = Path(__file__).with_name('private-paths.toml')


def private_path(key):
    try:
        data = tomllib.loads(FILE.read_text())
    except FileNotFoundError:
        raise SystemExit(f'{FILE} is missing; this real-title check needs {key!r}') from None
    if key not in data:
        raise SystemExit(f'{FILE} has no {key!r}; add it to run this real-title check')
    return Path(data[key])
