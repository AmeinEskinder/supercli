#!/usr/bin/env python3
"""Send Ctrl+Enter to the supercli gpuidart window via raw X11.

No xdotool/XTest available in this environment, so this speaks the X11
protocol directly over the Unix socket:
  1. Finds the top-level window titled `supercli` (via _NET_WM_NAME/WM_NAME).
  2. Sets input focus to it.
  3. Sends KeyPress/KeyRelease for Control_L + Return (state=ControlMask)
     with SendEvent, which is how a window manager-less Xvfb delivers keys.

Usage:
  python3 x11_ctrl_enter.py --display :99 --title supercli [--deny]
  --deny sends Ctrl+Shift+Enter instead.
"""
import argparse
import os
import socket
import struct
import sys
import time

X11_OP_QUERY_TREE = 15
X11_OP_INTERN_ATOM = 16
X11_OP_GET_PROPERTY = 20
X11_OP_SEND_EVENT = 25
X11_OP_SET_INPUT_FOCUS = 42
X11_OP_GET_KEYBOARD_MAPPING = 101

EV_KEY_PRESS = 2
EV_KEY_RELEASE = 3
CONTROL_MASK = 0x04
SHIFT_MASK = 0x01


class X11:
    def __init__(self, display):
        if display.startswith(":"):
            num = display[1:].split(".")[0]
            path = f"/tmp/.X11-unix/X{num}"
        else:
            raise ValueError(f"only local displays supported, got {display!r}")
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.connect(path)
        self.sock.settimeout(10)
        # Setup request: little-endian, X11R6-ish.
        self.sock.sendall(struct.pack("<BBHHHHH", 0x6C, 0, 11, 0, 0, 0, 0))
        # Setup reply: 8-byte prefix, then length*4 bytes of additional data
        # (the length counts from byte 8 of the reply). Additional data:
        # release(4) rid_base(4) rid_mask(4) motion_buf(4)
        # vendor_len(2) max_req(2) num_roots(1) num_formats(1)
        # img_order(1) bit_order(1) scan_unit(1) scan_pad(1)
        # min_keycode(1) max_keycode(1) unused(4)
        # then vendor(vendor_len, padded) + pixmap formats (8 bytes each)
        # + roots; each root starts with the root window id (4 bytes).
        prefix = self._read_exact(8)
        status = prefix[0]
        if status == 0:  # failed
            reason_len = prefix[1]
            reason = self._read_exact((reason_len + 3) & ~3)
            raise RuntimeError(f"X11 setup failed: {reason!r}")
        if status != 1:
            raise RuntimeError(f"X11 setup returned status {status}")
        data_len_words = struct.unpack("<H", prefix[6:8])[0]
        data = self._read_exact(data_len_words * 4)
        self.rid_base = struct.unpack("<I", data[4:8])[0]
        self.rid_mask = struct.unpack("<I", data[8:12])[0]
        self._rid_next = 0
        vendor_len = struct.unpack("<H", data[16:18])[0]
        num_formats = data[21]
        self.min_keycode = data[26]
        pos = 32 + ((vendor_len + 3) & ~3) + num_formats * 8
        self.root = struct.unpack_from("<I", data, pos)[0]
        self.seq = 0

    def _read_exact(self, n):
        buf = b""
        while len(buf) < n:
            chunk = self.sock.recv(n - len(buf))
            if not chunk:
                raise RuntimeError("X11 connection closed")
            buf += chunk
        return buf

    def new_id(self):
        # The mask portion must be nonzero: start the counter at 1.
        self._rid_next += 1
        return self.rid_base | (self._rid_next & self.rid_mask)

    def _request(self, opcode, payload=b"", reply=True, data=0):
        # X11 requests must be a whole number of 4-byte words. The second
        # header byte ("data") carries a per-request small field.
        payload += b"\x00" * ((4 - len(payload) % 4) % 4)
        nwords = (4 + len(payload)) // 4
        self.sock.sendall(struct.pack("<BBH", opcode, data, nwords) + payload)
        if not reply:
            return None
        header = self._read_exact(32)
        rtype = header[0]
        if rtype == 0:  # error
            code = header[1]
            raise RuntimeError(f"X11 error {code} on opcode {opcode}")
        length_words = struct.unpack("<I", header[4:8])[0]
        extra = self._read_exact(length_words * 4)
        # Return the full 32-byte reply header plus any extra data so call
        # sites can use the spec's header offsets directly.
        return header + extra

    def intern_atom(self, name, only_if_exists=False):
        # InternAtom: header data byte = only_if_exists; payload = nbytes(2)+pad(2)+name.
        payload = struct.pack("<H", len(name)) + b"\x00\x00"
        payload += name.encode() + b"\x00" * ((4 - len(name) % 4) % 4)
        rep = self._request(X11_OP_INTERN_ATOM, payload,
                            data=1 if only_if_exists else 0)
        # InternAtom reply: atom is the first CARD32 of the reply header body.
        return struct.unpack("<I", rep[8:12])[0]

    def get_property_string(self, window, atom, prop_type):
        # GetProperty: header data byte = delete(0); payload = window(4)
        # property(4) type(4) long_offset(4) long_length(4).
        payload = struct.pack("<IIIII", window, atom, prop_type, 0, 0xFFFFFFFF)
        rep = self._request(X11_OP_GET_PROPERTY, payload)
        # GetProperty reply header: [1]=format, [16:20]=nitems, value at [32:].
        fmt = rep[1]
        if fmt == 0:
            return None
        value_len = struct.unpack("<I", rep[16:20])[0]
        # format is 8 for the string types we request; guard anyway.
        nbytes = value_len * (fmt // 8)
        value = rep[32 : 32 + nbytes]
        try:
            return value.decode("utf-8", errors="replace").split("\x00")[0]
        except Exception:
            return None

    def query_tree(self, window):
        # QueryTree reply header: [16:18]=nchildren, children start at [32].
        rep = self._request(X11_OP_QUERY_TREE, struct.pack("<I", window))
        nchildren = struct.unpack("<H", rep[16:18])[0]
        children = struct.unpack("<%dI" % nchildren, rep[32 : 32 + 4 * nchildren])
        return children

    def find_window_by_title(self, title, timeout=30):
        atom_net_wm = self.intern_atom("_NET_WM_NAME")
        atom_utf8 = self.intern_atom("UTF8_STRING")
        atom_wm_name = self.intern_atom("WM_NAME")
        atom_string = self.intern_atom("STRING")
        deadline = time.time() + timeout

        def walk(w):
            name = self.get_property_string(w, atom_net_wm, atom_utf8)
            if name is None:
                name = self.get_property_string(w, atom_wm_name, atom_string)
            if name == title:
                return w
            for child in self.query_tree(w):
                found = walk(child)
                if found:
                    return found
            return None

        while time.time() < deadline:
            try:
                found = walk(self.root)
            except RuntimeError:
                found = None
            if found:
                return found
            time.sleep(0.5)
        return None

    def set_input_focus(self, window):
        # SetInputFocus: header data byte = revert_to(1=Parent); payload = focus(4).
        self._request(
            X11_OP_SET_INPUT_FOCUS, struct.pack("<I", window), reply=False, data=1
        )

    def keycode_for_keysym(self, keysym):
        # GetKeyboardMapping: header byte 1 unused; payload = first_keycode(1)
        # count(1) pad(2).
        count = 256 - self.min_keycode
        rep = self._request(
            X11_OP_GET_KEYBOARD_MAPPING,
            struct.pack("<BBH", self.min_keycode, count, 0),
        )
        # GetKeyboardMapping reply header: [1]=keysyms_per_keycode, keysyms at [32:].
        per = rep[1]
        syms = struct.unpack("<%dI" % (count * per), rep[32:])
        for i, s in enumerate(syms):
            if s == keysym:
                return self.min_keycode + i // per
        return None

    def send_key(self, window, keycode, press, state):
        ev_type = EV_KEY_PRESS if press else EV_KEY_RELEASE
        event = struct.pack(
            "<BBHIIIIhhhhHBB",
            ev_type,  # type
            keycode,  # detail
            0,  # sequence
            0,  # time (CurrentTime)
            self.root,  # root
            window,  # event
            0,  # child (None)
            0,  # root_x
            0,  # root_y
            0,  # event_x
            0,  # event_y
            state,  # state
            1,  # same_screen
            0,  # pad
        )
        # SendEvent: header(opcode, propagate, len) + dest(4) + mask(4) + event(32)
        body = struct.pack("<II", window, 0) + event
        nwords = (4 + len(body)) // 4
        self.sock.sendall(
            struct.pack("<BBH", X11_OP_SEND_EVENT, 0, nwords) + body
        )

    def close(self):
        self.sock.close()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--display", default=os.environ.get("DISPLAY", ":99"))
    ap.add_argument("--title", default="supercli")
    ap.add_argument("--deny", action="store_true",
                    help="send Ctrl+Shift+Enter instead of Ctrl+Enter")
    ap.add_argument("--probe", action="store_true",
                    help="only check that the window exists; exit 0 if found")
    ap.add_argument("--timeout", type=int, default=60)
    args = ap.parse_args()

    x = X11(args.display)
    print(f"connected to {args.display}, root=0x{x.root:x}", flush=True)
    win = x.find_window_by_title(args.title, timeout=args.timeout)
    if not win:
        print(f"ERROR: no window titled {args.title!r} appeared", flush=True)
        sys.exit(2)
    print(f"found window 0x{win:x}", flush=True)
    if args.probe:
        x.close()
        sys.exit(0)

    kc_ctrl = x.keycode_for_keysym(0xFFE3)  # Control_L
    kc_shift = x.keycode_for_keysym(0xFFE1)  # Shift_L
    kc_ret = x.keycode_for_keysym(0xFF0D)  # Return
    if kc_ctrl is None or kc_ret is None or (args.deny and kc_shift is None):
        print("ERROR: could not resolve keycodes", flush=True)
        sys.exit(3)
    print(f"keycodes: ctrl={kc_ctrl} shift={kc_shift} return={kc_ret}", flush=True)

    x.set_input_focus(win)
    time.sleep(0.3)
    mods = []
    if args.deny:
        mods.append((kc_shift, SHIFT_MASK))
    # press modifiers
    state = 0
    for kc, mask in mods:
        x.send_key(win, kc, True, state)
        state |= mask
        time.sleep(0.05)
    x.send_key(win, kc_ctrl, True, state)
    state |= CONTROL_MASK
    time.sleep(0.05)
    # Return with full modifier state
    x.send_key(win, kc_ret, True, state)
    time.sleep(0.15)
    x.send_key(win, kc_ret, False, state)
    time.sleep(0.05)
    # release modifiers
    x.send_key(win, kc_ctrl, False, state)
    state &= ~CONTROL_MASK
    for kc, mask in reversed(mods):
        x.send_key(win, kc, False, state)
        state &= ~mask
    print("sent " + ("Ctrl+Shift+Enter" if args.deny else "Ctrl+Enter"), flush=True)
    x.close()


if __name__ == "__main__":
    main()
