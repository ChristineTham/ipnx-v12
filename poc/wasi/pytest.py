# pytest.py: the third citizen's acceptance script — REAL CPython (wasi
# build) running a file out of the namespace. Prints are matched by init.
import os, sys, json
print("py: script ran on", sys.platform, "%d.%d" % sys.version_info[:2])
print("py: root:", " ".join(sorted(os.listdir("/"))[:5]))
open("/tmp/py.out", "w").write("written by python\n")
print("py: readback:", open("/tmp/py.out").read().strip())
d = json.loads(json.dumps({"kernel": "plan9", "n": 42}))
print("py: json:", d["kernel"], d["n"])
print("py: sum:", sum(range(100)))
