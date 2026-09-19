#!/usr/bin/env python3
"""Restore common-symbol semantics for rc.h's tentative definitions.

C89 lets a file write `int runq;` at file scope with no `extern`, and every
file that includes rc.h does. Those are TENTATIVE definitions and they MERGE —
one variable, however many files declared it, with at most one of them giving
it a value. kencc does this, and so does any compiler given `-fcommon`.

The wasm backend cannot. Measured on this machine, 2026-09-19:

    $ clang --target=wasm32-unknown-unknown -fcommon -c a.c
    error: common symbols are not yet implemented for Wasm: runq

Each tentative definition becomes an ordinary STRONG one, so the link fails
with a duplicate for every variable rc.h declares. Two things do not fix it:

  * `llvm-objcopy --weaken-symbol` — for wasm it supports "only flags for
    section dumping, removal, and addition".
  * `wasm-ld --allow-multiple-definition` — it keeps the FIRST definition,
    even over a later initialised one (measured: two objects, one holding
    `int Rcmain = 7;` and one holding `int Rcmain;`, link one way round and
    the answer is 7, the other way round and it is 0). Ordering the objects
    cannot fix it either, because two different files each initialise
    something: `Rcmain` and `Fdprefix` in `ipnx.c`, `doprompt` in `lex.c`.
    Whichever goes first, the other loses its value silently — which is how
    `havefork = 1` was once beaten by a zero (CLAUDE.md).

So the weak bit is set here, which is what a common symbol resolves to: the
duplicates are allowed, a strong definition beats them all, and if there is no
strong one any of the zeroes will do.

The bit is `WASM_SYM_BINDING_WEAK` in the `linking` section's symbol table
(tool-conventions/Linking.md). It goes in a LEB128 field whose length cannot
change, because setting bit 0 of an even number never lengthens it — so the
file is edited in place and no section size moves.

    weaken.py <nm> <object>...
"""
import subprocess
import sys
from collections import Counter

WEAK = 0x01
UNDEFINED = 0x10
EXPLICIT_NAME = 0x40
SYMBOL_TABLE = 8
SEGMENT_INFO = 5
K_FUNCTION, K_DATA, K_GLOBAL, K_SECTION, K_TAG, K_TABLE = 0, 1, 2, 3, 4, 5


class Reader:
    def __init__(self, b, at=0):
        self.b = b
        self.at = at

    def byte(self):
        v = self.b[self.at]
        self.at += 1
        return v

    def leb(self):
        """An unsigned LEB128, with where it started and how long it was."""
        start = self.at
        v = shift = 0
        while True:
            c = self.byte()
            v |= (c & 0x7F) << shift
            shift += 7
            if not c & 0x80:
                return v, start, self.at - start

    def u32(self):
        return self.leb()[0]

    def name(self):
        n = self.u32()
        s = self.b[self.at : self.at + n].decode("utf-8", "replace")
        self.at += n
        return s


def leb(v, width):
    """Re-encode `v` in exactly `width` bytes, or fail."""
    out = bytearray()
    while True:
        c = v & 0x7F
        v >>= 7
        if v:
            out.append(c | 0x80)
        else:
            out.append(c)
            break
    if len(out) != width:
        raise ValueError(f"flags would need {len(out)} bytes, not {width}")
    return bytes(out)


def symbols(b):
    """Every data symbol in the linking section.

    name -> (flags, at, width, initialised) — where `initialised` says the
    symbol's bytes are in a `.data` segment rather than a `.bss` one, which is
    exactly the difference between `int Rcmain = "/bin/rcmain";` and the
    tentative `char *Rcmain;` that every other file wrote.
    """
    r = Reader(b, 8)  # past "\0asm" and the version
    while r.at < len(b):
        sid = r.byte()
        size, _, _ = r.leb()
        end = r.at + size
        if sid == 0 and r.name() == "linking":
            return table(Reader(b, r.at), end)
        r.at = end
    return {}


def table(r, end):
    r.leb()  # the linking section's own version
    found, segments = {}, []
    while r.at < end:
        sub = r.byte()
        size, _, _ = r.leb()
        stop = r.at + size
        if sub == SEGMENT_INFO:
            for _ in range(r.u32()):
                segments.append(r.name())
                r.leb()  # alignment
                r.leb()  # flags
        elif sub == SYMBOL_TABLE:
            for _ in range(r.u32()):
                kind = r.byte()
                flags, at, width = r.leb()
                if kind == K_DATA:
                    name = r.name()
                    seg = None
                    if not flags & UNDEFINED:
                        seg = r.u32()
                        r.leb(), r.leb()  # offset, size
                    found[name] = (flags, at, width, seg)
                elif kind == K_SECTION:
                    r.leb()
                else:
                    r.leb()  # index
                    if not (flags & UNDEFINED) or flags & EXPLICIT_NAME:
                        r.name()
        r.at = stop
    return {
        name: (flags, at, width,
               seg is not None and not segments[seg].startswith(".bss"))
        for name, (flags, at, width, seg) in found.items()
    }


def main(nm, objs):
    defs = {}
    for o in objs:
        out = subprocess.run([nm, o], capture_output=True, text=True, check=True).stdout
        defs[o] = {
            f[2]
            for f in (line.split() for line in out.splitlines())
            if len(f) == 3 and f[1].isupper() and f[1] != "U"
        }
    count = Counter(n for s in defs.values() for n in s)
    dup = {n for n, c in count.items() if c > 1}
    if not dup:
        return

    # Read every object's symbol table once, and find, for each duplicated
    # name, the ONE object that gives it a value. That one keeps its strong
    # definition and wins the link; the rest are weakened.
    syms = {o: symbols(bytearray(open(o, "rb").read())) for o in objs}
    owner = {}
    for o in objs:
        for name in dup & defs[o]:
            s = syms[o].get(name)
            if s is not None and s[3]:
                if name in owner:
                    raise SystemExit(
                        f"weaken.py: {name} is initialised in both "
                        f"{owner[name]} and {o}"
                    )
                owner[name] = o

    for o in objs:
        here = dup & defs[o]
        if not here:
            continue
        b = bytearray(open(o, "rb").read())
        changed = False
        for name in here:
            if owner.get(name) == o:
                continue  # the one definition with a value
            s = syms[o].get(name)
            if s is None:
                continue  # a function, not one of rc.h's variables
            flags, at, width, _ = s
            if flags & WEAK:
                continue
            b[at : at + width] = leb(flags | WEAK, width)
            changed = True
        if changed:
            open(o, "wb").write(b)


main(sys.argv[1], sys.argv[2:])
