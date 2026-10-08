// The built root's index, which the browser host's tree starts from
// (hosts/web/src/tree.rs): one line a file, `kind perm size mtime path`,
// tab-separated — `d` or `f`, the permission in octal, the size in bytes,
// the modification time in seconds, the path relative to the root — each
// directory before what is in it.
//
//   node hosts/web/index.mjs userspace/root > root.index

import fs from 'node:fs';
import path from 'node:path';

const root = process.argv[2];
if (!root) {
  console.error('usage: node index.mjs root');
  process.exit(1);
}
const out = [];
function walk(rel) {
  for (const name of fs.readdirSync(path.join(root, rel)).sort()) {
    if (/[\t\n]/.test(name)) continue;
    const r = rel ? `${rel}/${name}` : name;
    const st = fs.statSync(path.join(root, r));
    const perm = (st.mode & 0o777).toString(8).padStart(4, '0');
    const mtime = Math.floor(st.mtimeMs / 1000);
    if (st.isDirectory()) {
      out.push(`d\t${perm}\t0\t${mtime}\t${r}`);
      walk(r);
    } else if (st.isFile()) {
      out.push(`f\t${perm}\t${st.size}\t${mtime}\t${r}`);
    }
  }
}
walk('');
process.stdout.write(out.join('\n') + '\n');
