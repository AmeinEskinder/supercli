#!/usr/bin/env python3
"""Neutralize the spurious __mh_execute_header definition in
crates/supercli-core/vendor/ghostty-vt/macos-universal/libghostty-vt.a.

Root cause of the relay-conformance macOS link failure:
  libghostty-vt-static_zcu.o (the zig-compiled VT engine inside the vendored
  archive) defines the external symbol __mh_execute_header. That symbol is
  reserved for the linker, which defines it for every executable. As long as
  the zig object is never extracted, linking succeeds -- which is why normal
  `cargo build` works (vt.o, the C API shim, only needs highway/simdutf).
  But `cargo test -p supercli-core --lib` compiles the
  ghostty_vt::tests::layout_matches_type_json unit test, which calls
  ghostty_type_json() -- defined only in the zig object. Extracting it pulls
  in the spurious __mh_execute_header, producing:
      ld: duplicate symbol '__mh_execute_header'

Fix: clear the N_EXT bit on that one nlist_64 entry in each fat slice,
demoting it to a local symbol. The linker then ignores it, the duplicate
disappears, and nothing that referenced it breaks (nothing does -- it is
purely a zig codegen artifact; the value points into the object's own
__TEXT).

The original file is backed up to <path>.bak. Idempotent.
"""
import struct
import sys

MH_EXECUTE_HEADER = b"__mh_execute_header"


def process_slice(sl):
    assert sl[:8] == b"!<arch>\n"
    pos = 8
    changed = 0
    out = bytearray(sl[:8])
    while pos < len(sl):
        hdr = sl[pos : pos + 60]
        name = hdr[0:16]
        size = int(hdr[48:58].decode("ascii").strip())
        content = bytearray(sl[pos + 60 : pos + 60 + size])
        # Only touch the zig object (name is via BSD long-name table '/19')
        if _real_name(name, sl) == "libghostty-vt-static_zcu.o":
            changed += _neutralize(content)
        out += hdr + bytes(content)
        if size % 2:
            out += b"\n"
        pos += 60 + size + (size % 2)
    return bytes(out), changed


def _real_name(name_field, sl):
    # Resolve BSD long names via the '//' string table (first member).
    n = name_field.decode("ascii", "replace").rstrip()
    if n == "//":
        return "//"
    if n.startswith("/"):
        # find strtab: parse first member
        sz0 = int(sl[8 + 48 : 8 + 58].decode("ascii").strip())
        strtab = sl[8 + 60 : 8 + 60 + sz0]
        idx = int(n[1:])
        end = strtab.find(b"\n", idx)
        return strtab[idx:end].decode("ascii", "replace").rstrip("/")
    return n.rstrip("/")


def _neutralize(obj):
    """Clear N_EXT on __mh_execute_header's nlist_64 entry. Returns count."""
    if struct.unpack("<I", obj[:4])[0] != 0xFEEDFACF:
        return 0
    ncmds = struct.unpack("<I", obj[16:20])[0]
    p = 32
    changed = 0
    for _ in range(ncmds):
        cmd, cmdsize = struct.unpack("<II", obj[p : p + 8])
        if cmd == 0x2:  # LC_SYMTAB
            symoff, nsyms, stroff, strsize = struct.unpack("<IIII", obj[p + 8 : p + 24])
            strs = obj[stroff : stroff + strsize]
            for i in range(nsyms):
                so = symoff + i * 16
                strx = struct.unpack("<I", obj[so : so + 4])[0]
                if strx >= strsize:
                    continue
                end = strs.find(b"\x00", strx)
                if strs[strx:end] == MH_EXECUTE_HEADER:
                    n_type = obj[so + 4]
                    if n_type & 0x01:  # N_EXT set
                        obj[so + 4] = n_type & ~0x01
                        changed += 1
        p += cmdsize
    return changed


def main(path):
    with open(path, "rb") as f:
        data = f.read()
    assert data[:4] == b"\xca\xfe\xba\xbe", "not a fat binary"
    nfat = struct.unpack(">I", data[4:8])[0]
    archs = []
    off = 8
    for _ in range(nfat):
        cputype, cpusubtype, aoff, asize, align = struct.unpack(">IIIII", data[off : off + 20])
        archs.append((cputype, cpusubtype, aoff, asize, align))
        off += 20

    out = bytearray(data[: 8 + 20 * nfat])
    total_changed = 0
    # Slices keep their offsets/sizes (we only flip bits in place).
    for cputype, cpusubtype, aoff, asize, align in archs:
        sl = data[aoff : aoff + asize]
        new_sl, changed = process_slice(sl)
        assert len(new_sl) == len(sl), "slice size must not change"
        total_changed += changed
        arch = "x86_64" if cputype == 0x1000007 else "arm64"
        print(f"  {arch}: neutralized {changed} symbol(s)")
        # splice back at the same offset
        while len(out) < aoff:
            out += b"\x00"
        assert len(out) == aoff
        out += new_sl
    # copy any trailing bytes
    last_end = max(a[2] + a[3] for a in archs)
    out += data[len(out) : last_end] if len(out) < last_end else b""
    out += data[last_end:]

    with open(path + ".bak", "wb") as f:
        f.write(data)
    with open(path, "wb") as f:
        f.write(bytes(out))
    print(f"done: {total_changed} symbol(s) neutralized; backup at {path}.bak")


if __name__ == "__main__":
    main(sys.argv[1])
