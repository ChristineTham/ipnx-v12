#!/usr/bin/env python3
"""
Plan 9's C, as clang compiles it.

Plan 9's source is written for kencc, whose language is ANSI C with a few
extensions, all defined in *How to Use the Plan 9 C Compiler*
(`plan9/sys/doc/comp.ms`):

  * `const` and `volatile` *"are also ignored"* (comp.ms:256) — `-Dconst=`;
  * an unnamed member — `Lock;`, `struct Lock;` — has its members
    *"addressable without prefix in the outer structure"* (:1111);
  * such a member *"may be accessed by type name if (and only if) [it is]
    declared using a typedef name"*: `mc->Mouse` (:1155);
  * *"the address of a struct Node may be used without a cast anywhere that
    the address of a struct Lock is used … The compiler automatically
    promotes the type and adjusts the address"*: `lock(node)` (:1144).

clang has the first half of the second (`-fms-extensions`) and none of the
rest, and it takes the last as a warning: it passes the unadjusted address,
which is right only when the member happens to come first.

This makes the difference up, as a derivation. The vendored text is never
touched: each file is written into build/kencc/ with

  1. every unnamed member `T;` as `union { T; T T; };` — the members of T
     still addressable without prefix, and the member itself by its type
     name. Where a member of T has the same name as a member of the outer
     structure, the outer one is the one kencc finds, so T is then only
     named, `T T;`, and a use of one of its members is written through it;
  2. every place clang reports a pointer conversion kencc would have
     promoted — passing, assigning, initialising, returning — written as
     the adjusted address, `&(E)->T`;
  3. the smaller differences as clang reports them: a prototype that names
     two parameters the same (kencc ignores the names), and a member reached
     through a structure that is only named.
  4. **a string literal is writable data.** kencc puts every literal into
     `.string`, an ordinary static (`cc/lex.c:1263`), a byte at a time as
     `ADATA` (`8c/swt.c:106`, `outstring`) — nothing read-only, and each
     literal its own bytes — and Plan 9's code writes into them: `ed.c:159`
     passes `"/tmp/eXXXXX"` to its own `mktemp`, which fills in the Xs.
     clang makes a literal a `constant` and, having inlined `mktemp`,
     deletes the stores. So a file is compiled to IR unoptimised, its
     literals made plain globals, and only then optimised ([`cc`]).

Steps 2 and 3 are driven by clang's own diagnostics, which give the exact
place and both types, and repeat until clang has nothing more to say.
"""
import os, re, subprocess, threading
from collections import ChainMap

HERE = os.path.dirname(os.path.abspath(__file__))
DERIVED = None           # build/kencc, set by init()

# ---------------------------------------------------------------------------
# a C lexer, just enough: identifiers, punctuation, and what to skip

TOK = re.compile(r"""
    (?P<ws>\s+)
  | (?P<com>/\*.*?\*/|//[^\n]*)
  | (?P<pp>^[ \t]*\#(?:[^\n\\]|\\.)*)
  | (?P<str>"(?:[^"\\\n]|\\.)*"|'(?:[^'\\\n]|\\.)*')
  | (?P<id>[A-Za-z_][A-Za-z_0-9]*)
  | (?P<num>[0-9][A-Za-z0-9_.]*)
  | (?P<p>.)
""", re.X | re.S | re.M)

def tokens(text):
    for m in TOK.finditer(text):
        k = m.lastgroup
        if k in ("ws", "com", "pp"):
            continue
        yield k, m.group(), m.start(), m.end()

KEYWORDS = set("""auto break case char const continue default do double else enum
extern float for goto if int long register return short signed sizeof static
struct switch typedef union unsigned void volatile while""".split())

# ---------------------------------------------------------------------------
# records: what each structure holds

class Record:
    def __init__(self, tag, key):
        self.tag = tag            # struct tag, or None
        self.key = key            # "tag" or "unnamed at file:line:col"
        self.fields = []          # named members, in order
        self.unnamed = []         # (typename, flattened)
        self.first = None         # the first member: a field's name, or an unnamed one's type
    def __repr__(self):
        return f"Record({self.key}, {self.fields}, {self.unnamed})"

class Table:
    """Records by tag, by typedef name, and by where an unnamed one is."""
    def __init__(self):
        self.bytag = {}
        self.typedef = {}         # name -> tag, or name -> Record for typedef struct {..} Name
        self.bypos = {}
    def record(self, name):
        name = name.strip()
        m = re.match(r"^(?:struct|union)\s+\(unnamed at (.*)\)$", name)
        if m:
            return self.bypos.get(m.group(1))
        m = re.match(r"^(?:struct|union)\s+(\w+)$", name)
        if m:
            return self.bytag.get(m.group(1))
        seen = set()
        while name in self.typedef and name not in seen:
            seen.add(name)
            t = self.typedef[name]
            if isinstance(t, Record):
                return t
            if t in self.bytag:
                return self.bytag[t]
            name = t
        return self.bytag.get(name)
    def allfields(self, rec, seen=None):
        """Every name addressable without prefix in rec."""
        seen = seen or set()
        if rec is None or id(rec) in seen:
            return set()
        seen.add(id(rec))
        out = set(rec.fields)
        for t, flat in rec.unnamed:
            if flat:
                out |= self.allfields(self.record(t), seen)
        return out
    def path(self, frm, to, seen=None):
        """The unnamed members from record frm down to one of type `to` —
        kencc's promotion — as member names, or None."""
        seen = seen or set()
        if frm is None or id(frm) in seen:
            return None
        seen.add(id(frm))
        for t, _ in frm.unnamed:
            r = self.record(t)
            if r is not None and r is to:
                return [t]
        for t, _ in frm.unnamed:
            p = self.path(self.record(t), to, seen)
            if p:
                return [t] + p
        return None
    def fieldpath(self, frm, field, seen=None):
        """How a member name is reached through members that are named only."""
        seen = seen or set()
        if frm is None or id(frm) in seen:
            return None
        seen.add(id(frm))
        for t, flat in frm.unnamed:
            r = self.record(t)
            if r is None:
                continue
            if field in self.allfields(r):
                return [t] if not flat else None
            p = self.fieldpath(r, field, seen)
            if p:
                return [t] + p
        return None

def linecol(text, off):
    line = text.count("\n", 0, off) + 1
    col = off - (text.rfind("\n", 0, off) + 1) + 1
    return line, col

def parse(text, path, table, edits=None):
    """Read the records in a file into the table. With `edits`, also say how
    each unnamed member is to be written (the offsets of `T;`)."""
    toks = list(tokens(text))
    stack = []     # (kind, Record or None, token index of '{')
    decl = []      # tokens of the member declaration being read
    i = 0
    last_closed = None
    while i < len(toks):
        k, v, s, e = toks[i]
        if v in ("struct", "union") and i + 1 < len(toks):
            j = i + 1
            tag = toks[j][1] if toks[j][0] == "id" else None
            if tag:
                j += 1
            if j < len(toks) and toks[j][1] == "{":
                line, col = linecol(text, s)
                rec = Record(tag, tag or f"{path}:{line}:{col}")
                if tag:
                    table.bytag[tag] = rec
                else:
                    table.bypos[f"{path}:{line}:{col}"] = rec
                if stack and stack[-1][0] == "record":
                    decl.append(("rec", rec, s, e))
                stack.append(("record", rec, decl))
                decl = []
                i = j + 1
                continue
            if stack and stack[-1][0] == "record":
                decl.append(toks[i])
            i += 1
            continue
        if v == "{":
            if stack and stack[-1][0] == "record":
                decl.append(toks[i])
            stack.append(("block", None, decl))
            decl = []
            i += 1
            continue
        if v == "}":
            if not stack:
                i += 1
                continue
            kind, rec, saved = stack.pop()
            if kind == "record":
                # an anonymous nested record: `union { ... };` directly
                if stack and stack[-1][0] == "record":
                    decl = saved
                    # its fields flatten into the parent unless a declarator follows
                    nxt = toks[i + 1][1] if i + 1 < len(toks) else ";"
                    if nxt == ";" and rec.tag is None:
                        parent = stack[-1][1]
                        parent.fields.extend(rec.fields)
                        parent.unnamed.extend(rec.unnamed)
                else:
                    decl = saved
                    # `typedef struct {..} Name;`
                    j = i + 1
                    if j < len(toks) and toks[j][0] == "id":
                        # find whether this was a typedef
                        if any(t[1] == "typedef" for t in saved[-3:]) or _typedef_before(toks, i):
                            table.typedef[toks[j][1]] = rec
                last_closed = rec
            else:
                decl = saved
                if stack and stack[-1][0] == "record":
                    decl.append(toks[i])
            i += 1
            continue
        if stack and stack[-1][0] == "record":
            if v == ";":
                member(decl, stack[-1][1], text, edits)
                decl = []
            else:
                decl.append(toks[i])
            i += 1
            continue
        if not stack and v == "typedef":
            # typedef struct Tag Name;  typedef Other Name;
            j = i + 1
            words = []
            while j < len(toks) and toks[j][1] not in (";", "{"):
                words.append(toks[j][1])
                j += 1
            if j < len(toks) and toks[j][1] == ";" and len(words) >= 2:
                name = words[-1]
                if words[0] in ("struct", "union") and len(words) == 3:
                    table.typedef[name] = words[1]
                elif len(words) == 2 and re.match(r"^\w+$", words[0]):
                    table.typedef[name] = words[0]
        i += 1

def _typedef_before(toks, close_i):
    # walk back to the matching '{' and see whether `typedef` precedes it
    depth = 0
    for j in range(close_i, -1, -1):
        v = toks[j][1]
        if v == "}":
            depth += 1
        elif v == "{":
            depth -= 1
            if depth == 0:
                for t in toks[max(0, j - 4):j]:
                    if t[1] == "typedef":
                        return True
                return False
    return False

def member(decl, rec, text, edits):
    """One member declaration of a record."""
    # a declaration that starts with a structure body — `struct {..} x;` —
    # names its members after it; only a lone type name is unnamed
    body = any(t[0] == "rec" for t in decl)
    decl = [t for t in decl if t[0] != "rec"]
    words = [t[1] for t in decl]
    if not body and len(words) == 1 and decl[0][0] == "id" and words[0] not in KEYWORDS:
        if rec.first is None and not rec.fields and not rec.unnamed:
            rec.first = words[0]
        rec.unnamed.append((words[0], True))
        if edits is not None:
            edits.append((rec, words[0], decl[0][2], decl[0][3]))
        return
    if not body and len(words) == 2 and words[0] in ("struct", "union") and decl[1][0] == "id":
        if rec.first is None and not rec.fields and not rec.unnamed:
            rec.first = words[1]
        rec.unnamed.append((words[1], True))
        if edits is not None:
            edits.append((rec, words[1], decl[0][2], decl[1][3], words[0]))
        return
    # named members: split at top-level commas
    depth = 0
    cur = []
    parts = []
    for t in decl:
        if t[1] in "([":
            depth += 1
        elif t[1] in ")]":
            depth -= 1
        if t[1] == "," and depth == 0:
            parts.append(cur)
            cur = []
        else:
            cur.append(t)
    parts.append(cur)
    for p in parts:
        name = None
        vs = [t[1] for t in p]
        for a in range(len(vs) - 2):
            if vs[a] == "(" and vs[a + 1] == "*" and re.match(r"^\w+$", vs[a + 2]):
                name = vs[a + 2]
                break
        if name is None:
            for t in p:
                if t[1] in ("[", ":"):
                    break
                if t[0] == "id" and t[1] not in KEYWORDS:
                    name = t[1]
        if name:
            if rec.first is None and not rec.fields and not rec.unnamed:
                rec.first = name
            rec.fields.append(name)

# ---------------------------------------------------------------------------
# step 1: the unnamed members

ARCHS = "386 68000 68020 alpha amd64 arm arm64 mips mips64 power power64 riscv riscv64 sparc sparc64 spim wasm".split()

def absinclude(m):
    # `#include "/sys/src/cmd/lex/ldefs.h"`, `#include "/mips/include/ureg.h"`:
    # a Plan 9 absolute path, which is this tree's
    p = m.group(2)
    top = p.split("/")[1]
    if top == "sys" or top in ARCHS:
        return f'{m.group(1)}"{os.path.join(DERIVED, p[1:])}"'
    return m.group(0)

HEX = set("0123456789abcdefABCDEF")

def literals(text):
    """kencc's literals, as clang takes them (`cc/lex.c`, `escchar`):

      * `\\x` takes at most two hex digits, six in a wide string (`:1104`,
        *"note this is not ansi, supposed to only accept 2 hex"*); clang
        takes every hex digit that follows. A string that goes on with one
        is split there, `"\\xe2" "abc"`, which is the same bytes.
      * three quotes are a quote character; clang wants it escaped.
      * an empty element in a list — `Gpiogpo1en = 0x04, /* gpio1
        enable */,` (`usb/ether/asix.c:45`) — which kencc takes; no C has
        two commas in a row, so the second goes.
      * *"all multibyte runes are alpha"* (`:459`, `:732`): an identifier
        may hold any rune past ASCII, and clang takes only Unicode's
        identifier characters — `OS½` (`ip/ftpfs/ftpfs.c:75`). Outside a
        literal or a comment such a rune is spelled `_U00BD_`, the same in
        every file.
    """
    Q, D, B = "'", '"', "\\"
    out, i, n = [], 0, len(text)
    sig = ""        # the last character outside a literal, a comment or a blank
    while i < n:
        c = text[i]
        if text.startswith("//", i):
            j = text.find("\n", i)
            j = n if j < 0 else j
            out.append(text[i:j])
            i = j
        elif text.startswith("/*", i):
            j = text.find("*/", i + 2)
            j = n if j < 0 else j + 2
            out.append(text[i:j])
            i = j
        elif text.startswith(Q * 3, i):
            out.append(Q + B + Q + Q)
            sig = Q
            i += 3
        elif c == Q:
            j = i + 1
            while j < n and text[j] != Q and text[j] != "\n":
                j += 2 if text[j] == B else 1
            out.append(text[i:j + 1])
            sig = Q
            i = j + 1
        elif c == D:
            wide = i > 0 and text[i - 1] == "L"
            limit = 6 if wide else 2
            j = i + 1
            buf = [D]
            while j < n and text[j] != D and text[j] != "\n":
                if text[j] == B and j + 1 < n and text[j + 1] == "x":
                    k = j + 2
                    while k < n and k - (j + 2) < limit and text[k] in HEX:
                        k += 1
                    buf.append(text[j:k])
                    if k < n and text[k] in HEX:
                        buf.append(D + " " + ("L" if wide else "") + D)
                    j = k
                elif text[j] == B:
                    buf.append(text[j:j + 2])
                    j += 2
                else:
                    buf.append(text[j])
                    j += 1
            if j < n and text[j] == D:
                buf.append(D)
                j += 1
            out.append("".join(buf))
            sig = D
            i = j
        elif ord(c) > 0x7f:
            out.append("_U%04X_" % ord(c))
            sig = "_"
            i += 1
        elif c == "," and sig == ",":
            i += 1
        else:
            out.append(c)
            if c not in " \t\n":
                sig = c
            i += 1
    return "".join(out)

FLOATL = re.compile(r"""(//[^\n]*|/\*.*?\*/|"(?:\\.|[^"\\\n])*"|'(?:\\.|[^'\\\n])*')|(?<![\w.])((?:\d+\.\d*|\.\d+)(?:[eE][+-]?\d+)?|\d+[eE][+-]?\d+)[lL]\b""", re.S)

def floatl(text):
    """A floating constant's `L` is read and ignored — *"if(c == 'L' || c ==
    'l') { … c1 |= Numlong; }"*, and the constant is `LDCONST`, a double
    (`cc/lex.c:913`) — where clang makes it a long double (`cifs/dfs.c:89`,
    `1000.0L`)."""
    return FLOATL.sub(lambda m: m.group(1) or m.group(2), text)

def mainfix(text):
    """`main` as `_main` calls it — for every program, derived or not
    (`boot` and `args` are compiled by mk.sh as they are)."""
    # **`main(void)` is given what `_main` passes it.** kencc's calls pass
    # arguments whatever the callee declares — `_main` calls `main(argc,
    # argv)` (`libc/386/main9.s`) and a `main(void)` ignores them. A wasm
    # call must match the callee's signature, and clang names a `main(void)`
    # `__main_void`, so `_main` finds no `main` at all.
    # APE's programs spell it `int main(void)` on one line, and `main()`.
    text = re.sub(r'^((?:int|void)[ \t]+)?main[ \t]*\([ \t]*(?:void)?[ \t]*\)', r'\1main(int, char**)', text, flags=re.M)
    # **And `main` answers an int.** kencc's `_main` takes whatever is in
    # the return register (`libc/386/main9.s`, `ape/lib/ap/386/main9.s`:
    # *"CALL main(SB); MOVL AX, 0(SP); CALL exit(SB)"*), so a Plan 9
    # program's `void main` and an APE program's `int main` are called
    # alike. A wasm call must match the callee's type, and one type must be
    # the entry's: `int`, because APE's `exit(main(…))` carries the status a
    # `return` gives. A `void main` falls off its end, as on 386.
    # — and its declarations, `extern void main(int, char*[]);` (`units.y:61`)
    text = re.sub(r'^((?:extern[ \t]+)?)void([ \t]*\n?[ \t]*)main([ \t]*\()', r'\1int\2main\3', text, flags=re.M)
    return text


def derive_text(text, path, table):
    """The file as clang takes it: each unnamed member flattened and named,
    or only named where a member of it would be hidden by the outer one."""
    text = re.sub(r'^([ \t]*#[ \t]*include[ \t]*)[<"](/[^">]+)[">]', absinclude, text, flags=re.M)
    text = mainfix(text)
    # **`long double` is `double`**: *"case BDOUBLE|BLONG: return
    # types[TDOUBLE]"* (`cc/sub.c:264`). clang's is 128 bits on wasm32,
    # with arithmetic in a runtime library this machine does not have
    # (`cifs`: `__divtf3`), and cannot be told otherwise there
    text = re.sub(r"\blong(\s+)double\b", r"double", text)
    text = floatl(text)
    text = literals(text)
    edits = []
    local = Table()
    local.bytag = ChainMap({}, *table.bytag.maps) if isinstance(table.bytag, ChainMap) else table.bytag
    local.typedef = ChainMap({}, *table.typedef.maps) if isinstance(table.typedef, ChainMap) else table.typedef
    local.bypos = ChainMap({}, *table.bypos.maps) if isinstance(table.bypos, ChainMap) else table.bypos
    parse(text, path, local, edits)
    derive_text.local = local
    out = []
    last = 0
    taken = {}      # per record: what a name already finds
    for ed in edits:
        rec, t, s, e = ed[0], ed[1], ed[2], ed[3]
        kw = ed[4] + " " if len(ed) > 4 else ""
        inner = local.record(kw + t if kw else t)
        # a name that is no record — a macro, `BZ_RAND_DECLS;`
        # (`bzip2/lib/bzlib_private.h:215`), which expands to members — is
        # left as it is written
        if inner is None:
            for i, (n, f) in enumerate(rec.unnamed):
                if n == t:
                    rec.unnamed[i] = (n, False)
            continue
        if id(rec) not in taken:
            taken[id(rec)] = set(rec.fields)
        # kencc: the outer structure's own members are found first, then the
        # unnamed members' in order — so a name an earlier one has is its
        mine = local.allfields(inner) if inner is not None else set()
        hidden = bool(mine & taken[id(rec)])
        if not hidden:
            taken[id(rec)] |= mine
        # after text up to `T;` — the `;` follows e
        semi = text.index(";", e)
        out.append(text[last:s])
        if hidden:
            out.append(f"{kw}{t} {t}/*kencc: unnamed, hidden*/;")
            for i, (n, f) in enumerate(rec.unnamed):
                if n == t:
                    rec.unnamed[i] = (n, False)
        else:
            out.append(f"union {{ {kw}{t}; {kw}{t} {t}; }};")
        last = semi + 1
    out.append(text[last:])
    return "".join(out)

# ---------------------------------------------------------------------------
# the tree

_table = None
_lock = threading.Lock()

def headers_of(d):
    return [os.path.join(d, f) for f in sorted(os.listdir(d)) if f.endswith(".h")]

# **A file sees the records its headers declare, and no others** — as the
# compiler does. One table for the whole tree let a tag another program
# declares stand in for the one a file includes: `drawterm`'s `QLock`
# (`cmd/unix`, which Plan 9 does not build: `BUGGERED=unix`,
# `cmd/mkfile:11`) for libc's, acme's `Window` for rio's.
INCLUDE = re.compile(r'^[ \t]*#[ \t]*include[ \t]*([<"])([^>"]+)[>"]', re.M)
_own = {}       # a file -> the Table of the records it declares
_texts = {}
_views = {}     # a derived file -> the Table it compiles against

def incdirs(p):
    """Where `<x.h>` is looked for: APE's headers for APE's sources
    (`cmd/pcc.c:163`), libc's for the rest, and the other after them."""
    libc = [os.path.join(HERE, "wasm", "include"), os.path.join(HERE, "sys", "include")]
    ape = [os.path.join(HERE, "wasm", "include", "ape"), os.path.join(HERE, "sys", "include", "ape")]
    if os.sep + os.path.join("sys", "src", "ape") + os.sep in p:
        return ape + libc
    return libc + ape

def includes(p):
    """The headers a file includes, and theirs, in the order they come."""
    out, seen, todo = [], {p}, [p]
    while todo:
        f = todo.pop(0)
        text = _texts.get(f)
        if text is None:
            continue
        for kind, name in INCLUDE.findall(text):
            if name.startswith("/"):
                top = name.split("/")[1]
                cands = [os.path.join(HERE, name[1:])] if top == "sys" or top in ARCHS else []
            else:
                dirs = ([os.path.dirname(f)] if kind == '"' else []) + incdirs(p)
                cands = [os.path.normpath(os.path.join(d, name)) for d in dirs]
            for q in cands:
                if q in _texts:
                    if q not in seen:
                        seen.add(q)
                        out.append(q)
                        todo.append(q)
                    break
    return out

def view(p):
    """The table a file is derived and compiled against: its own records,
    then those of the headers it includes, then — for a header found
    through a directory this cannot see, a `-I` of the mkfile's — the
    tree's."""
    chain = [p] + includes(p)
    t = Table()
    t.bytag = ChainMap(*[_own[q].bytag for q in chain if q in _own], _table.bytag)
    t.typedef = ChainMap(*[_own[q].typedef for q in chain if q in _own], _table.typedef)
    t.bypos = ChainMap(*[_own[q].bypos for q in chain if q in _own], _table.bypos)
    return t

def init(build, roots):
    """Derive every C file and header under `roots` into build/kencc, with
    step 1 applied, and build the record table the later steps use."""
    global DERIVED, _table
    DERIVED = os.path.join(build, "kencc")
    _table = Table()
    files = []
    for root in roots:
        for dp, dn, fn in os.walk(root):
            for f in fn:
                if f.endswith((".c", ".h", ".y")):
                    files.append(os.path.join(dp, f))
    # and what a file includes by a name with none of those endings —
    # `#include "macbody"` (`cc/mac.c`), C by another name
    more = set()
    for p in files:
        d = os.path.dirname(p)
        with open(p, errors="replace") as f:
            for kind, name in INCLUDE.findall(f.read()):
                q = os.path.normpath(os.path.join(d, name))
                if kind == '"' and not name.endswith((".c", ".h", ".y")) and os.path.isfile(q):
                    more.add(q)
    files += sorted(more - set(files))
    # headers first, so a .c's records see the types they embed
    files.sort(key=lambda p: (not p.endswith(".h"), p))
    texts = _texts
    for p in files:
        with open(p, errors="replace") as f:
            texts[p] = f.read()
        parse(texts[p], derived(p), _table)
        own = Table()
        parse(texts[p], derived(p), own)
        _own[p] = own
    for p in files:
        dst = derived(p)
        if p.endswith(".y"):
            new = texts[p]
        else:
            v = view(p)
            new = derive_text(texts[p], dst, v)
            # what the file declares, as its derivation decided each
            # unnamed member — which the files that include it see
            loc = derive_text.local
            own = Table()
            own.bytag, own.typedef, own.bypos = loc.bytag.maps[0], loc.typedef.maps[0], loc.bypos.maps[0]
            _own[p] = own
            _views[dst] = loc
        os.makedirs(os.path.dirname(dst), exist_ok=True)
        if not os.path.exists(dst) or open(dst, errors="replace").read() != new:
            with open(dst, "w") as f:
                f.write(new)
    # the table is the one step 1 filled: its records carry which unnamed
    # members are flattened and which only named

def derived(p):
    rel = os.path.relpath(p, HERE)
    return os.path.join(DERIVED, rel)

# ---------------------------------------------------------------------------
# steps 2 and 3: what clang says

DIAG = re.compile(r"^(?P<file>[^:\n]+):(?P<line>\d+):(?P<col>\d+):(?P<ranges>(?:\{\d+:\d+-\d+:\d+\})*):? (?P<kind>warning|error|note): (?P<msg>.*)$", re.M)
CONV = re.compile(r"incompatible pointer types (?:passing|assigning to|initializing|returning) '([^']*)'(?: \(aka '[^']*'\))? (?:to parameter of type|from|with an expression of type|from a function with result type) '([^']*)'")

VCONV = re.compile(r"(?:assigning to|initializing) '([^'*]*)'(?: \(aka '[^']*'\))? (?:from|with an expression of) incompatible type '([^'*]*)'")

def ptrtarget(t):
    t = t.strip()
    if not t.endswith("*") or t.endswith("**"):
        return None
    return t[:-1].strip()

def offset(text, line, col):
    off = 0
    for _ in range(line - 1):
        off = text.index("\n", off) + 1
    return off + col - 1

def declspan(text, off):
    """The declaration around offset `off`: from after the previous `;`,
    `}` or preprocessor line, to the `;` or `{` that ends its declarator —
    answered as (start, end-of-declarator)."""
    s = off
    while s > 0 and text[s - 1] not in ";}":
        s -= 1
        if text[s] == "\n" and text[s + 1:s + 2] == "#":
            break
    while s < off and text[s] in " \t\n":
        s += 1
    while text.startswith("#", s):
        s = text.index("\n", s) + 1
        while s < off and text[s] in " \t\n":
            s += 1
    depth, e = 0, off
    while e < len(text):
        c = text[e]
        if c == "(":
            depth += 1
        elif c == ")":
            depth -= 1
            if depth == 0:
                return s, e + 1
        e += 1
    return s, e

def fixes(diags, table):
    """Edits to make, as (file, start, end, replacement-fn)."""
    ed = []
    for n, d in enumerate(diags):
        msg = d["msg"]
        if d["kind"] == "note":
            continue
        # `.fd 0` — the older designator, *"The Plan 9 compiler accepts either
        # form"* (comp.ms:1206)
        if msg.startswith("expected '=' or another designator"):
            l, c = d["line"], d["col"]
            ed.append((d["file"], (l, c), (l, c), lambda s: "= "))
            continue
        # a function declared `static` in a block, which kencc takes
        if msg.startswith("function declared in block scope cannot have 'static'"):
            l, c = d["line"], d["col"]
            ed.append((d["file"], (l, c), (l, c + len("static")), lambda s: ""))
            continue
        # a redeclaration kencc allows — an enum parameter declared int, a
        # pointer to a different structure: the definition is what is
        # compiled, so the earlier declaration is written as it
        m = re.match(r"conflicting types for '(\w+)'", msg)
        if m and n + 1 < len(diags) and diags[n + 1]["kind"] == "note" and diags[n + 1]["msg"].startswith("previous declaration"):
            prev = diags[n + 1]
            ed.append(("redecl", d, prev, m.group(1)))
            continue
        m = CONV.search(msg)
        if m and d["ranges"]:
            a, b = m.group(1), m.group(2)
            # "assigning to B from A", "initializing B with ... A",
            # "returning A from ... B", "passing A to ... B"
            if msg.startswith("incompatible pointer types assigning to") or msg.startswith("incompatible pointer types initializing"):
                to, frm = a, b
            else:
                frm, to = a, b
            tf, tt = ptrtarget(frm), ptrtarget(to)
            if tf is None or tt is None:
                continue
            rf, rt = table.record(tf), table.record(tt)
            if rf is None or rt is None or rf is rt:
                continue
            p = table.path(rf, rt)
            if not p:
                continue
            # **a first member is at the structure's own address**, so the
            # pointer is already the one kencc's promotion gives — `Biobuf*`
            # for `Biobufhdr*` (`bio.h:39`) — and clang's is a warning only.
            # Rewriting it anyway reached into macro arguments (`assert`) and
            # into headers two compiles were editing at once
            r, leading = rf, True
            for t in p:
                if r is None or r.first != t:
                    leading = False
                    break
                r = table.record(t)
            if leading:
                continue
            (l1, c1, l2, c2) = d["ranges"][-1]
            ed.append((d["file"], (l1, c1), (l2, c2), lambda s, p=p: f"(&({s})->{'.'.join(p)})"))
            continue
        # the same promotion for a structure itself: `*s = res;` with `Store
        # *s` and `Node res` (`acid/builtin.c:1309`) assigns res's unnamed
        # `Store`
        m = VCONV.match(msg)
        if m and d["ranges"]:
            rf, rt = table.record(m.group(2)), table.record(m.group(1))
            p = table.path(rf, rt) if rf is not None and rt is not None and rf is not rt else None
            if p:
                (l1, c1, l2, c2) = d["ranges"][-1]
                ed.append((d["file"], (l1, c1), (l2, c2), lambda s, p=p: f"({s}).{'.'.join(p)}"))
            continue
        m = re.match(r"no member named '(\w+)' in '([^']*)'", msg)
        if m:
            rec = table.record(m.group(2))
            p = table.fieldpath(rec, m.group(1)) if rec else None
            if p:
                l, c = d["line"], d["col"]
                ed.append((d["file"], (l, c), (l, c + len(m.group(1))),
                           lambda s, p=p, f=m.group(1): ".".join(p) + "." + f))
            continue
        # a block's `extern char lastc;` against the file's `extern int
        # lastc;` (`db/setup.c:102`, `db/defs.h:110`), which kencc takes:
        # the block's is written as the file's
        m = re.match(r"redeclaration of '(\w+)' with a different type: '([^']*)' vs '([^']*)'", msg)
        if m:
            l, c, new, old = d["line"], d["col"], m.group(2), m.group(3)
            ed.append((d["file"], (l, 1), (l, c), lambda s, new=new, old=old: s[::-1].replace(new[::-1], old[::-1], 1)[::-1]))
            continue
        # `extern struct symtab symtab[];` where the structure is not
        # defined (`grap/grapl.lx:12`, declared and not used): kencc takes an
        # extern array of an incomplete type, C does not. The declaration
        # goes; if anything used it, what uses it now says so
        if msg.startswith("array has incomplete element type"):
            try:
                line = open(d["file"]).read().split("\n")[d["line"] - 1]
            except (OSError, IndexError):
                continue
            if re.fullmatch(r"\s*extern\s[^;{}]*\[\s*\]\s*;\s*", line):
                l = d["line"]
                ed.append((d["file"], (l, 1), (l, len(line) + 1),
                           lambda s: "/* kencc: an extern array of an incomplete type: " + s.strip().replace("*/", "") + " */"))
            continue
        m = re.match(r"redefinition of parameter '(\w+)'", msg)
        if m:
            l, c = d["line"], d["col"]
            n = m.group(1)
            ed.append((d["file"], (l, c), (l, c + len(n)), lambda s, n=n, l=l, c=c: f"{n}_{l}_{c}"))
    return ed

def apply(edits):
    byfile = {}
    for e in edits:
        if e[0] == "redecl":
            _, d, prev, name = e
            if not (d["file"].startswith(DERIVED) and prev["file"].startswith(DERIVED)):
                continue
            with open(d["file"]) as fh:
                cur = fh.read()
            s, t = declspan(cur, offset(cur, d["line"], d["col"]))
            sig = re.sub(r"\s+", " ", cur[s:t]).strip()
            sig = re.sub(r"^(static|extern)\s+", "", sig)
            byfile.setdefault(prev["file"], []).append(("sig", prev, sig))
            continue
        f, a, b, fn = e
        byfile.setdefault(f, []).append((a, b, fn))
    changed = 0
    for f, eds in byfile.items():
        if not f.startswith(DERIVED):
            continue
        with open(f) as fh:
            text = fh.read()
        spans = []
        for x in eds:
            if x[0] == "sig":
                _, prev, sig = x
                s, t = declspan(text, offset(text, prev["line"], prev["col"]))
                old = text[s:t]
                kw = re.match(r"^(static|extern)\s+", old)
                new = (kw.group(0) if kw else "") + sig
                spans.append((s, t, lambda _s, new=new: new))
                continue
            a, b, fn = x
            s = offset(text, *a)
            e = offset(text, *b)        # clang's range end is one past
            spans.append((s, e, fn))
        spans.sort(key=lambda x: (x[0], x[1]), reverse=True)
        lastS = None
        for s, e, fn in spans:
            if lastS is not None and e > lastS:
                continue            # overlapping: another round
            text = text[:s] + fn(text[s:e]) + text[e:]
            lastS = s
            changed += 1
        with open(f, "w") as fh:
            fh.write(text)
    return changed

STRLIT = re.compile(r"^(@\.str[.\w]*) = private unnamed_addr constant ", re.M)


def cc(cc, flags, src, out, extra=()):
    """Compile as kencc would, literals writable (step 4). Answers the
    front end's CompletedProcess, whose diagnostics are clang's own."""
    ll = out + ".ll"
    r = subprocess.run([cc] + flags + list(extra) + ["-Xclang", "-disable-llvm-passes", "-S", "-emit-llvm",
                       src, "-o", ll], capture_output=True, text=True)
    if r.returncode == 0:
        with open(ll) as f:
            t = f.read()
        with open(ll, "w") as f:
            f.write(STRLIT.sub(r"\1 = internal global ", t))
        o = [x for x in flags if x.startswith(("--target", "-O", "-m"))]
        b = subprocess.run([cc] + o + ["-c", ll, "-o", out], capture_output=True, text=True)
        if b.returncode != 0:
            r = b
        os.remove(ll)
    return r


def compile(cc_, flags, src, out, rounds=8):
    """Compile a derived file, fixing what kencc would have accepted, until
    clang has nothing more to say. Answers clang's errors, or None."""
    diagflags = ["-fno-caret-diagnostics", "-fdiagnostics-print-source-range-info",
                 "-fno-color-diagnostics", "-Wincompatible-pointer-types"]
    for _ in range(rounds):
        r = cc(cc_, flags, src, out, diagflags)
        diags = []
        for m in DIAG.finditer(r.stderr):
            rs = [tuple(map(int, re.split(r"[:\-]", x))) for x in re.findall(r"\{([^}]*)\}", m.group("ranges"))]
            diags.append({"file": m.group("file"), "line": int(m.group("line")), "col": int(m.group("col")),
                          "ranges": rs, "kind": m.group("kind"), "msg": m.group("msg")})
        eds = fixes(diags, _views.get(src, _table))
        if eds:
            with _lock:
                if apply(eds):
                    continue
        if r.returncode == 0:
            return None
        errs = [d for d in diags if d["kind"] == "error"]
        return errs[0]["msg"] if errs else r.stderr.strip().split("\n")[0]
    return "kencc: did not settle"


if __name__ == "__main__":
    # `kencc.py cc <src> <obj> [flag...]`, for mk.sh: $CC and $CFLAGS from
    # the environment
    import sys
    if len(sys.argv) < 4 or sys.argv[1] != "cc":
        sys.exit("usage: kencc.py cc src obj [flag...]")
    src = sys.argv[2]
    text = open(src, errors="replace").read()
    if mainfix(text) != text:
        # a program: its `main` as `_main` calls it, from a copy beside the
        # object; `-I` keeps its own directory's headers
        src = sys.argv[3] + ".c"
        with open(src, "w") as f:
            f.write(mainfix(text))
        sys.argv.append("-I" + os.path.dirname(os.path.abspath(sys.argv[2])))
    r = cc(os.environ["CC"], os.environ["CFLAGS"].split() + sys.argv[4:], src, sys.argv[3])
    sys.stderr.write(r.stderr)
    sys.exit(r.returncode)
