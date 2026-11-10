#!/usr/bin/python3
"""Compositor-level keyboard and wheel input for the private KWin test session.

KWin's org.kde.KWin.EIS.RemoteDesktop D-Bus interface hands out an emulated
input (EIS) socket; libei sends events through it. The events travel through
the compositor like device input, unlike QtTest events posted inside the
application. Only used inside desktop_session.py's private bus and compositor;
it is not a physical keyboard or wheel.
"""
import ctypes
import select
import time

import dbus

POINTER, POINTER_ABSOLUTE, KEYBOARD, TOUCH, SCROLL, BUTTON = (1 << n for n in range(6))
EVENT_DISCONNECT, EVENT_SEAT_ADDED, EVENT_DEVICE_ADDED = 2, 3, 5
KEYS = {'up': 103, 'down': 108, 'left': 105, 'right': 106, 'page_up': 104, 'page_down': 109, 'home': 102, 'end': 107,
        'tab': 15, 'escape': 1, 'space': 57, 'shift': 42, 'backspace': 14}
# US-layout key positions; the compositor keymap decides the resulting symbol.
POSITIONS = dict(zip('qwertyuiop', range(16, 26))) | dict(zip('asdfghjkl', range(30, 39))) \
    | dict(zip('zxcvbnm', range(44, 51))) | dict(zip('1234567890', range(2, 12))) \
    | {' ': 57, '/': 53, '-': 12, '.': 52, "'": 40, '`': 41}
SHIFTED = {'"': "'", '~': '`'}

lib = ctypes.CDLL('libei.so.1')
for name, restype, argtypes in (
        ('ei_new_sender', ctypes.c_void_p, [ctypes.c_void_p]),
        ('ei_configure_name', None, [ctypes.c_void_p, ctypes.c_char_p]),
        ('ei_setup_backend_fd', ctypes.c_int, [ctypes.c_void_p, ctypes.c_int]),
        ('ei_dispatch', ctypes.c_int, [ctypes.c_void_p]),
        ('ei_get_event', ctypes.c_void_p, [ctypes.c_void_p]),
        ('ei_get_fd', ctypes.c_int, [ctypes.c_void_p]),
        ('ei_event_get_type', ctypes.c_int, [ctypes.c_void_p]),
        ('ei_event_unref', ctypes.c_void_p, [ctypes.c_void_p]),
        ('ei_event_get_seat', ctypes.c_void_p, [ctypes.c_void_p]),
        ('ei_event_get_device', ctypes.c_void_p, [ctypes.c_void_p]),
        ('ei_seat_has_capability', ctypes.c_int, [ctypes.c_void_p, ctypes.c_uint]),
        ('ei_device_has_capability', ctypes.c_int, [ctypes.c_void_p, ctypes.c_uint]),
        ('ei_device_ref', ctypes.c_void_p, [ctypes.c_void_p]),
        ('ei_device_start_emulating', None, [ctypes.c_void_p, ctypes.c_uint32]),
        ('ei_device_frame', None, [ctypes.c_void_p, ctypes.c_uint64]),
        ('ei_device_keyboard_key', None, [ctypes.c_void_p, ctypes.c_uint32, ctypes.c_int]),
        ('ei_device_pointer_motion_absolute', None, [ctypes.c_void_p, ctypes.c_double, ctypes.c_double]),
        ('ei_device_scroll_discrete', None, [ctypes.c_void_p, ctypes.c_int32, ctypes.c_int32])):
    function = getattr(lib, name)
    function.restype = restype
    function.argtypes = argtypes


class Input:
    def __init__(self, timeout=5.0):
        interface = dbus.Interface(dbus.SessionBus().get_object('org.kde.KWin', '/org/kde/KWin/EIS/RemoteDesktop'),
                                   'org.kde.KWin.EIS.RemoteDesktop')
        result = interface.connectToEIS(dbus.Int32(POINTER | POINTER_ABSOLUTE | KEYBOARD | SCROLL | BUTTON))
        self.context = lib.ei_new_sender(None)
        lib.ei_configure_name(self.context, b'rustris-test')
        if lib.ei_setup_backend_fd(self.context, result[0].take()) != 0:
            raise RuntimeError('Cannot attach to the compositor input socket')
        self.pointer = self.keyboard = None
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline and not (self.pointer and self.keyboard):
            select.select([lib.ei_get_fd(self.context)], [], [], .2)
            lib.ei_dispatch(self.context)
            while event := lib.ei_get_event(self.context):
                kind = lib.ei_event_get_type(event)
                if kind == EVENT_DISCONNECT:
                    raise RuntimeError('The compositor closed the input socket')
                if kind == EVENT_SEAT_ADDED:
                    seat = lib.ei_event_get_seat(event)
                    capabilities = [ctypes.c_uint(c) for c in (POINTER, POINTER_ABSOLUTE, KEYBOARD, SCROLL, BUTTON)
                                    if lib.ei_seat_has_capability(seat, c)]
                    lib.ei_seat_bind_capabilities(ctypes.c_void_p(seat), *capabilities, ctypes.c_void_p(None))
                elif kind == EVENT_DEVICE_ADDED:
                    device = lib.ei_event_get_device(event)
                    if not self.pointer and lib.ei_device_has_capability(device, POINTER_ABSOLUTE) \
                            and lib.ei_device_has_capability(device, SCROLL):
                        self.pointer = lib.ei_device_ref(device)
                    if not self.keyboard and lib.ei_device_has_capability(device, KEYBOARD):
                        self.keyboard = lib.ei_device_ref(device)
                lib.ei_event_unref(event)
        if not (self.pointer and self.keyboard):
            raise RuntimeError('The compositor offered no keyboard and scrolling pointer')
        for device in {self.pointer, self.keyboard}:
            lib.ei_device_start_emulating(device, 0)
        lib.ei_dispatch(self.context)

    def _frame(self, device):
        lib.ei_device_frame(device, int(time.monotonic()*1_000_000))
        lib.ei_dispatch(self.context)

    def press_code(self, code, shift=False, hold=.01):
        if shift:
            lib.ei_device_keyboard_key(self.keyboard, KEYS['shift'], 1); self._frame(self.keyboard)
        lib.ei_device_keyboard_key(self.keyboard, code, 1); self._frame(self.keyboard)
        time.sleep(hold)
        lib.ei_device_keyboard_key(self.keyboard, code, 0); self._frame(self.keyboard)
        if shift:
            lib.ei_device_keyboard_key(self.keyboard, KEYS['shift'], 0); self._frame(self.keyboard)
        time.sleep(hold)

    def key(self, name, hold=.01):
        self.press_code(KEYS[name], hold=hold)

    def strokes(self, sequence):
        """Press the US-layout positions for each character, shifted for capitals and '"' or '~'."""
        for character in sequence:
            base = SHIFTED.get(character, character.lower())
            self.press_code(POSITIONS[base], shift=character in SHIFTED or character.isupper())

    def move(self, x, y):
        lib.ei_device_pointer_motion_absolute(self.pointer, x, y); self._frame(self.pointer)

    def wheel(self, steps):
        """Positive steps scroll down, one discrete detent (120 units) each."""
        lib.ei_device_scroll_discrete(self.pointer, 0, 120*steps); self._frame(self.pointer)
