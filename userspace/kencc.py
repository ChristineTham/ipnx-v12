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
        rec.unnamed.append((words[0], True))
        if edits is not None:
            edits.append((rec, words[0], decl[0][2], decl[0][3]))
        return
    if not body and len(words) == 2 and words[0] in ("struct", "union") and decl[1][0] == "id":
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
    """
    Q, D, B = "'", '"', "\\"
    out, i, n = [], 0, len(text)
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
            i += 3
        elif c == Q:
            j = i + 1
            while j < n and text[j] != Q and text[j] != "\n":
                j += 2 if text[j] == B else 1
            out.append(text[i:j + 1])
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
            i = j
        else:
            out.append(c)
            i += 1
    return "".join(out)

def derive_text(text, path, table):
    """The file as clang takes it: each unnamed member flattened and named,
    or only named where a member of it would be hidden by the outer one."""
    text = re.sub(r'^([ \t]*#[ \t]*include[ \t]*)[<"](/[^">]+)[">]', absinclude, text, flags=re.M)
    # **`main(void)` is given what `_main` passes it.** kencc's calls pass
    # arguments whatever the callee declares — `_main` calls `main(argc,
    # argv)` (`libc/386/main9.s`) and a `main(void)` ignores them. A wasm
    # call must match the callee's signature, and clang names a `main(void)`
    # `__main_void`, so `_main` finds no `main` at all.
    text = re.sub(r'^main\(void\)', 'main(int, char**)', text, flags=re.M)
    text = literals(text)
    edits = []
    local = Table()
    local.bytag, local.typedef, local.bypos = table.bytag, table.typedef, table.bypos
    parse(text, path, local, edits)
    out = []
    last = 0
    taken = {}      # per record: what a name already finds
    for ed in edits:
        rec, t, s, e = ed[0], ed[1], ed[2], ed[3]
        kw = ed[4] + " " if len(ed) > 4 else ""
        inner = local.record(kw + t if kw else t)
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
    # headers first, so a .c's records see the types they embed
    files.sort(key=lambda p: (not p.endswith(".h"), p))
    texts = {}
    for p in files:
        with open(p, errors="replace") as f:
            texts[p] = f.read()
        parse(texts[p], derived(p), _table)
    for p in files:
        dst = derived(p)
        new = derive_text(texts[p], dst, _table) if not p.endswith(".y") else texts[p]
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
            (l1, c1, l2, c2) = d["ranges"][-1]
            ed.append((d["file"], (l1, c1), (l2, c2), lambda s, p=p: f"(&({s})->{'.'.join(p)})"))
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
        eds = fixes(diags, _table)
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
    r = cc(os.environ["CC"], os.environ["CFLAGS"].split() + sys.argv[4:], sys.argv[2], sys.argv[3])
    sys.stderr.write(r.stderr)
    sys.exit(r.returncode)
