#!/usr/bin/env python3
"""
Build Plan 9's libraries and commands from their own mkfiles.

Plan 9 builds with mk, and every library and command it has says in its
mkfile what it is made of: `</sys/src/cmd/mksyslib` and `LIB=`/`OFILES=` for
a library, `</sys/src/cmd/mkone` and `TARG=` for a program, `mkmany` for
several, `DIRS=` for the directories below. This reads those declarations as
mk does — continuation lines, `<` includes, assignments, `${VAR:a%b=c%d}`,
backquotes — and builds what they declare with this machine's compiler and
loader. It does not run mk's recipes: they are rc, and each is one of a few
kinds (compile, archive, link, yacc) this knows how to do for wasm.

Nothing is left out on purpose. What does not build is written to
`build/failed` with its reason, which is the work that is left.

usage: mkfile.py libs|cmds  (the environment comes from mk.sh)
"""
import os, re, subprocess, sys, glob, shutil
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import kencc
from concurrent.futures import ThreadPoolExecutor

HERE = os.path.dirname(os.path.abspath(__file__))
SYS = os.path.join(HERE, "sys")
BUILD = os.environ.get("build", os.path.join(HERE, "build"))
PKG = os.environ["pkg"]
ROOT = os.environ["root"]
OBJTYPE = os.environ.get("OBJTYPE", "wasm")
CC = os.environ["CC"]
LD = os.environ["LD"]
NM = os.path.join(os.path.dirname(CC), "llvm-nm")
AR = os.environ["AR"]
CFLAGS = os.environ["CFLAGS"].split()
LDFLAGS = os.environ["LDFLAGS"].split()
LIBDIR = os.path.join(BUILD, "lib")        # /$objtype/lib
# Plan 9's C as kencc takes it (kencc.py): const is ignored (comp.ms:256),
# and headers come from the derived tree
KFLAGS = [f for f in CFLAGS if not f.startswith("-I")] + ["-Dconst=",
    "-I" + os.path.join(BUILD, "kencc", OBJTYPE, "include"),
    "-I" + os.path.join(BUILD, "kencc", "sys", "include")]
FAILED = os.path.join(BUILD, "failed")
JOBS = os.cpu_count() or 4

# ---------------------------------------------------------------------------
# reading a mkfile

def rootpath(p, env):
    """A Plan 9 absolute path, as this tree holds it."""
    p = expand(p, env, os.getcwd())[0] if "$" in p else p
    if p.startswith("/sys/"):
        return os.path.join(HERE, p[1:])
    if p.startswith("/" + OBJTYPE + "/"):
        return os.path.join(HERE, OBJTYPE, p[len(OBJTYPE) + 2:])
    return p

def reduce(args, cwd):
    # `reduce` (libc, libsec, libmp): the list less whatever the machine's
    # directory supplies — `ls -p ../$objtype/*.[cs] | sed 's/..$//'` as
    # patterns for `grep -v -f`, so a match anywhere in the name
    objtype, files = args[1], args[2:]
    mach = os.path.join(cwd, "..", objtype)
    pats = [f[:-2] for f in os.listdir(mach) if f.endswith((".c", ".s"))] if os.path.isdir(mach) else []
    return [f for f in files if not any(p in f for p in pats)]

def backquote(cmd, cwd):
    w = cmd.split()
    if w[:2] == ["rc", "./reduce"]:
        return reduce(w[2:], cwd)
    # rc's `{...}: the commands used in these mkfiles are ls, sed, echo, grep
    # and pwd, which sh runs the same; rc's `>[2]` is sh's `2>`.
    cmd = cmd.replace(">[2]", "2>")
    try:
        out = subprocess.run(["sh", "-c", cmd], cwd=cwd, capture_output=True, text=True, timeout=30).stdout
    except Exception:
        return []
    return out.split()

VARREF = re.compile(r"\$(\{[^}]*\}|[A-Za-z_][A-Za-z_0-9]*)")

def subst(words, pat):
    # ${VAR:a%b=c%d} — mk's pattern substitution
    a, b = pat.split("=", 1)
    if "%" not in a:
        return [w[:-len(a)] + b if a and w.endswith(a) else w for w in words]
    pa, sa = a.split("%", 1)
    pb, sb = (b.split("%", 1) + [""])[:2] if "%" in b else (b, None)
    out = []
    for w in words:
        if w.startswith(pa) and w.endswith(sa) and len(w) >= len(pa) + len(sa):
            stem = w[len(pa):len(w) - len(sa)] if sa else w[len(pa):]
            out.append(pb + stem + sb if sb is not None else pb)
        else:
            out.append(w)
    return out

def expand(text, env, cwd):
    """Expand a line into words: variables are lists, as in mk."""
    # backquotes first
    def bq(m):
        inner = " ".join(expand(m.group(1), env, cwd)) if "rc ./reduce" in m.group(1) else m.group(1)
        return " ".join(backquote(inner, cwd))
    text = re.sub(r"`\{([^}]*)\}", bq, text)
    words = []
    for tok in text.split():
        words.extend(expand_word(tok, env))
    return words

def expand_word(tok, env):
    m = VARREF.search(tok)
    if not m:
        return [tok]
    pre, post = tok[:m.start()], tok[m.end():]
    ref = m.group(1)
    if ref.startswith("{"):
        ref = ref[1:-1]
        if ":" in ref:
            name, pat = ref.split(":", 1)
            val = subst(env.get(name, []), " ".join(expand_word(pat, env)) if "$" in pat else pat)
        else:
            val = env.get(ref, [])
    else:
        val = env.get(ref, [])
    rest = expand_word(post, env) if "$" in post else [post]
    out = []
    if not val:
        # an empty list: what is around it stands alone, if anything is
        return [pre + r for r in rest if pre + r]
    for v in val:
        for r in rest:
            out.append(pre + v + r)
    return out

def logical_lines(path):
    # mk strips a comment from each physical line, and a line that ends in a
    # backslash still continues — `libmach/mkfile` has `#\t0\` in the middle
    # of FILES, and everything after it is still in the list
    with open(path, errors="replace") as f:
        phys = f.read().split("\n")
    out, cur = [], ""
    for line in phys:
        cont = line.endswith("\\")
        body = line[:-1] if cont else line
        if "#" in body and not body.lstrip().startswith("\t") and "'" not in body.split("#", 1)[0]:
            body = body.split("#", 1)[0]
        cur += body + (" " if cont else "")
        if not cont:
            out.append(cur)
            cur = ""
    if cur:
        out.append(cur)
    return out

class Mk:
    def __init__(self, dirpath, env):
        self.dir = dirpath
        self.env = dict(env)
        self.rules = []          # (targets, prereqs, recipe lines)
        self.includes = []       # template files included
        self.read(os.path.join(dirpath, "mkfile"))

    def read(self, path):
        lines = logical_lines(path)
        i = 0
        while i < len(lines):
            line = lines[i]
            i += 1
            s = line
            if not s.strip() or line.startswith("\t"):
                continue
            if s.startswith("<|"):
                continue
            if s.startswith("<"):
                inc = s[1:].strip()
                inc = " ".join(expand(inc, self.env, self.dir))
                self.includes.append(inc)
                p = rootpath(inc, self.env) if inc.startswith("/") else os.path.join(self.dir, inc)
                if os.path.isfile(p):
                    self.read(p)
                continue
            m = re.match(r"^([A-Za-z_][A-Za-z_0-9]*)\s*=(?!=)(.*)$", s)
            if m and ":" not in s[:s.index("=")]:
                name, val = m.group(1), m.group(2)
                self.env[name] = expand(val, self.env, self.dir)
                continue
            m = re.match(r"^([^:=]+?):([A-Za-z]*:)?(.*)$", s)
            if m:
                recipe = []
                while i < len(lines) and (lines[i].startswith("\t") or lines[i].startswith(" ")):
                    recipe.append(lines[i].strip())
                    i += 1
                tg = expand(m.group(1), self.env, self.dir)
                pr = expand(m.group(3), self.env, self.dir)
                self.rules.append((tg, pr, recipe))

    def get(self, name):
        return self.env.get(name, [])

    def uses(self, template):
        return any(t.endswith(template) for t in self.includes)

# ---------------------------------------------------------------------------
# building

failures = []

def fail(unit, why):
    failures.append(f"{unit}: {why}")

def cflags_for(mk):
    """The mkfile's CFLAGS, as this compiler takes them: -D and -I kept (a
    Plan 9 path mapped into this tree), kencc's own switches dropped."""
    out = []
    for f in mk.get("CFLAGS"):
        if f.startswith("-D"):
            out.append(f)
        elif f.startswith("-I"):
            p = f[2:]
            d = rootpath(p, mk.env) if p.startswith("/") else os.path.join(mk.dir, p)
            out.append("-I" + (kencc.derived(d) if d.startswith(HERE) else d))
    return out

def objdir(mk):
    rel = os.path.relpath(mk.dir, SYS)
    d = os.path.join(BUILD, "obj", rel)
    os.makedirs(d, exist_ok=True)
    return d

def source_of(mk, obj):
    """Where an object comes from: x.c, or the parser a YFILES makes."""
    base = obj[:-2] if obj.endswith(".o") else obj
    if base == "y.tab":
        return os.path.join(kencc.derived(mk.dir), "y.tab.c")
    c = os.path.join(mk.dir, base + ".c")
    if os.path.exists(c):
        return c
    # a source a recipe makes from another: `%.c:D: %.mp` (libsec/port),
    # made by running the recipe on this system
    for tg, pr, recipe in mk.rules:
        if "%.c" in tg and recipe:
            for p in pr:
                if "%" in p:
                    q = os.path.join(mk.dir, p.replace("%", os.path.basename(base)))
                    if os.path.exists(q):
                        return generate(mk, base, q, recipe)
    # a metarule naming where the source is: `%.$O: ../cc/%.c` (`8c/mkfile`)
    for tg, pr, _ in mk.rules:
        if "%.o" in tg:
            for p in pr:
                if p.endswith("%.c"):
                    q = os.path.normpath(os.path.join(mk.dir, p.replace("%", os.path.basename(base))))
                    if os.path.exists(q):
                        return q
    # `(bc|units|mpc).c:R: \1.tab.c` (`cmd/mkfile`): a program of one .y
    y = os.path.join(mk.dir, base + ".y")
    if os.path.exists(y):
        out = os.path.join(kencc.derived(mk.dir), base + ".c")
        if not os.path.exists(out) or os.path.getmtime(out) < os.path.getmtime(y):
            subprocess.run(["bison", "-y", "-d", "-o", out, y], capture_output=True, cwd=mk.dir)
        return out
    if os.path.exists(os.path.join(mk.dir, base + ".s")):
        return None  # Plan 9 assembler: this machine has none
    return c

import threading
GENLOCK = threading.Lock()

def generate(mk, base, prereq, recipe):
    """Make a source by running its mkfile's recipe — rc, with Plan 9's own
    commands — on the system this builds (`ipnx`), as Plan 9 builds itself
    with itself. The prerequisite goes into the store's /tmp, the recipe
    runs there, and the target comes back into the derived tree."""
    out = os.path.join(kencc.derived(mk.dir), base + ".c")
    if os.path.exists(out) and os.path.getmtime(out) >= os.path.getmtime(prereq):
        return out
    ipnx = os.environ.get("IPNX")
    if not ipnx or not os.path.exists(ipnx):
        return out           # not yet: the second pass, once ipnx is built
    stem = os.path.basename(base)
    # one system at a time, each in a directory of its own: the compiles
    # that ask for these run in parallel
    with GENLOCK:
        return _generate(prereq, recipe, stem, out, ipnx)

def _generate(prereq, recipe, stem, out, ipnx):
    name = "mkfile.py." + stem
    work = os.path.join(ROOT, "tmp", name)
    os.makedirs(work, exist_ok=True)
    shutil.copy(prereq, os.path.join(work, os.path.basename(prereq)))
    target = stem + ".c"
    script = "cd /tmp/%s; prereq=%s; target=%s; stem=%s\n%s\n" % (
        name, os.path.basename(prereq), target, stem, "\n".join(recipe))
    with open(os.path.join(work, "recipe.rc"), "w") as f:
        f.write(script)
    env = dict(os.environ, IPNX_STORE=ROOT)
    subprocess.run([ipnx, "rc", "/tmp/%s/recipe.rc" % name], env=env, capture_output=True,
                   stdin=subprocess.DEVNULL, timeout=300)
    made = os.path.join(work, target)
    if os.path.exists(made) and os.path.getsize(made) > 0:
        os.makedirs(os.path.dirname(out), exist_ok=True)
        shutil.move(made, out)
    shutil.rmtree(work, ignore_errors=True)
    return out

def yacc(mk):
    yf = mk.get("YFILES")
    if not yf:
        return True
    od = kencc.derived(mk.dir)
    os.makedirs(od, exist_ok=True)
    r = subprocess.run(["bison", "-y", "-d", "-o", os.path.join(od, "y.tab.c")] + [os.path.join(mk.dir, y) for y in yf],
                       capture_output=True, text=True, cwd=mk.dir)
    if r.returncode != 0:
        return False
    # Plan 9's yacc writes y.tab.h; some mkfiles call it x.tab.h
    shutil.copy(os.path.join(od, "y.tab.h"), os.path.join(od, "x.tab.h"))
    return True

def compile_obj(mk, obj, extra):
    src = source_of(mk, obj)
    od = objdir(mk)
    out = os.path.join(od, os.path.basename(obj))
    if src is None:
        return out, f"{obj} is assembler"
    # the kencc derivation of the file, or the file itself when it is one
    # this build generated (a parser from bison)
    dsrc = kencc.derived(src) if src.startswith(SYS) and not src.startswith(kencc.DERIVED) else src
    if os.path.exists(out) and os.path.exists(dsrc) and os.path.getmtime(out) >= os.path.getmtime(dsrc):
        return out, None
    flags = KFLAGS + ["-I" + kencc.derived(mk.dir), "-I" + od] + extra
    e = kencc.compile(CC, flags, dsrc, out)
    if e:
        return out, f"{os.path.basename(src)}: {e}"
    return out, None

def compile_all(mk, objs):
    extra = cflags_for(mk)
    with ThreadPoolExecutor(JOBS) as ex:
        res = list(ex.map(lambda o: compile_obj(mk, o, extra), objs))
    errs = [e for _, e in res if e]
    return [p for p, _ in res], errs

def archive(path, objs):
    # `ar vu $LIB` (mksyslib): members are added or replaced, never the
    # archive recreated — libsec, libmp and libc are each built from `port`
    # and then the machine's directory into the one library, and a member of
    # the machine's replaces the portable one of the same name
    os.makedirs(os.path.dirname(path), exist_ok=True)
    subprocess.run([AR, "rs", path] + objs, check=True, capture_output=True)

def syslibs():
    return sorted(glob.glob(os.path.join(LIBDIR, "*.a")))

def link(out, objs, locallibs):
    os.makedirs(os.path.dirname(out), exist_ok=True)
    # kencc's tentative definitions merge, as common symbols; the wasm
    # backend has none, so the weak bit is set instead (weaken.py)
    subprocess.run(["python3", os.path.join(HERE, "weaken.py"), NM] + objs, check=True, capture_output=True)
    cmd = [LD] + LDFLAGS + ["-o", out] + objs + locallibs + syslibs() + [os.path.join(BUILD, "libc.a")]
    r = subprocess.run(cmd, capture_output=True, text=True)
    if r.returncode != 0:
        und = sorted(set(re.findall(r"undefined symbol: (\S+)", r.stderr)))
        return "undefined " + " ".join(und[:12]) if und else (r.stderr.strip().split("\n")[0][:200])
    # asyncify: `setjmp`, `longjmp` and `fork` are the machine's (mk.sh)
    r = subprocess.run([os.environ["WASMOPT"]] + os.environ["ASYNCIFY"].split() + [out, "-o", out],
                       capture_output=True, text=True)
    if r.returncode != 0:
        return "asyncify: " + r.stderr.strip().split("\n")[0][:200]
    return None

def libpath(mk, lib):
    """A LIB= target: /$objtype/lib/libX.a is a system library; anything
    else is the directory's own, linked into its programs."""
    if lib.startswith("/" + OBJTYPE + "/lib/") or lib.startswith("/$objtype/lib/"):
        return os.path.join(LIBDIR, os.path.basename(lib))
    # a directory's own, where its mkfile says: `lib/lib.$O.a` is built by
    # `lib/mkfile` as `lib.$O.a`, and both name the one file
    return os.path.normpath(os.path.join(objdir(mk), lib))

# ---------------------------------------------------------------------------

BASEENV = {
    "objtype": [OBJTYPE], "cputype": [OBJTYPE], "O": ["o"],
    "APE": ["/sys/src/ape"], "SYS": [], "CPUS": [OBJTYPE],
}

def build_dir(d, want, rel):
    if not os.path.isfile(os.path.join(d, "mkfile")):
        return
    try:
        mk = Mk(d, BASEENV)
    except Exception as e:
        fail(rel, f"mkfile: {e}")
        return
    if want == "libs" and (mk.uses("mksyslib")):
        lib = mk.get("LIB")
        if not lib:
            return
        if not yacc(mk):
            fail(rel, "yacc failed")
            return
        objs, errs = compile_all(mk, mk.get("OFILES"))
        if errs:
            fail(rel, "; ".join(errs[:3]) + (f" (+{len(errs)-3} more)" if len(errs) > 3 else ""))
            return
        archive(libpath(mk, lib[0]), objs)
        return
    if want == "cmds":
        build_cmds(mk, d, rel)

def binpath(mk, name):
    # `$BIN/init: $O.init; cp $prereq /$objtype/init` (`cmd/mkfile:116`)
    if name == "init" and os.path.abspath(mk.dir) == os.path.join(SYS, "src", "cmd"):
        return os.path.join(ROOT, OBJTYPE, "init")
    b = mk.get("BIN") or ["/" + OBJTYPE + "/bin"]
    b = b[0]
    sub = b.split("/bin", 1)[1].strip("/") if "/bin" in b else ""
    return os.path.join(PKG, OBJTYPE, "bin", sub, name)

def build_cmds(mk, d, rel):
    local = []
    # a directory library (mklib), or LIB= naming one in the directory
    for lib in mk.get("LIB"):
        if lib.endswith(".a") and not lib.startswith("/"):
            local.append(libpath(mk, lib))
    if mk.uses("mklib") or (mk.uses("mksyslib") and mk.get("LIB") and not mk.get("LIB")[0].startswith("/")):
        objs, errs = compile_all(mk, mk.get("OFILES"))
        if errs:
            fail(rel, "; ".join(errs[:3]))
        else:
            archive(libpath(mk, mk.get("LIB")[0]), objs)
        return
    progs = {}   # name -> objects
    custom = {}
    for tg, pr, recipe in mk.rules:
        if any("$LD" in r or "LD" in r.split()[:1] for r in recipe):
            for t in tg:
                if t.startswith("o."):
                    custom[t[2:]] = [p for p in pr if p.endswith(".o")]
    targ = mk.get("TARG")
    # a rule with no recipe adds prerequisites to a target whose recipe is
    # elsewhere — mkmany's `$O.%: %.$O $OFILES` — as mk merges them
    # (`plumb/mkfile`: `$O.plumber: $PLUMBER`)
    more = {}
    for tg, pr, recipe in mk.rules:
        if not recipe:
            for t in tg:
                if t.startswith("o.") and t[2:] in targ:
                    more.setdefault(t[2:], []).extend(p for p in pr if p.endswith(".o"))
    if mk.uses("mkone") and targ:
        progs[targ[0]] = mk.get("OFILES")
    elif mk.uses("mkmany") or (targ and os.path.abspath(d) == os.path.join(SYS, "src", "cmd")):
        for t in targ:
            progs[t] = list(dict.fromkeys([t + ".o"] + mk.get("OFILES") + more.get(t, [])))
    for t, objs in custom.items():
        if t in progs or t in targ:
            progs[t] = objs
    if progs:
        if not yacc(mk):
            fail(rel, "yacc failed")
            return
        allobjs = sorted(set(o for objs in progs.values() for o in objs))
        paths, errs = compile_all(mk, allobjs)
        bad = set()
        for e in errs:
            bad.add(e.split(":", 1)[0])
        path_of = dict(zip(allobjs, paths))
        errmap = {}
        for o, p in zip(allobjs, paths):
            pass
        res = dict(zip(allobjs, [None] * len(allobjs)))
        for e in errs:
            src = e.split(":", 1)[0]
            for o in allobjs:
                if os.path.basename(source_of(mk, o) or o) == src or o == src:
                    res[o] = e
        for t, objs in sorted(progs.items()):
            broken = [res[o] for o in objs if res.get(o)]
            if broken:
                fail(f"{rel}/{t}" if rel != "cmd" else t, broken[0])
                continue
            e = link(binpath(mk, t), [path_of[o] for o in objs], local)
            if e:
                fail(f"{rel}/{t}" if rel != "cmd" else t, e)

def walk(top, want):
    rel = os.path.relpath(top, os.path.join(SYS, "src"))
    try:
        mk = Mk(top, BASEENV) if os.path.isfile(os.path.join(top, "mkfile")) else None
    except Exception:
        mk = None
    dirs = mk.get("DIRS") if mk else []
    # the portable directory and this machine's; another architecture's is
    # its own (`libmp/mkfile`: `for(i in port $objtype)`)
    dirs = [x for x in dirs if x not in kencc.ARCHS or x == OBJTYPE]
    # `for(i in cc $DIRS)` (`cmd/mkfile`): the compilers' common library first
    if "cc" in dirs:
        dirs = ["cc"] + [x for x in dirs if x != "cc"]
    # the directories below first: a program's own library is built in one
    # (`auth/lib`), and a library's portable half comes before its machine's
    for sub in dirs:
        p = os.path.join(top, sub)
        if os.path.isdir(p):
            walk(p, want)
    build_dir(top, want, rel)

def main():
    want = sys.argv[1]
    skip = set(sys.argv[2:])
    os.makedirs(LIBDIR, exist_ok=True)
    src = os.path.join(SYS, "src")
    archinc = [os.path.join(HERE, a, "include") for a in kencc.ARCHS if os.path.isdir(os.path.join(HERE, a, "include"))]
    kencc.init(BUILD, [os.path.join(SYS, "include"), src] + archinc)
    if want == "libs":
        for d in sorted(glob.glob(os.path.join(src, "lib*"))):
            if os.path.basename(d) in skip:
                continue
            walk(d, "libs")
    else:
        walk(os.path.join(src, "cmd"), "cmds")
    with open(FAILED, "a") as f:
        for x in failures:
            f.write(x + "\n")
    print(f"mkfile.py {want}: {len(failures)} did not build")

main()
