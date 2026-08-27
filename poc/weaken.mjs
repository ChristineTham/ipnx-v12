// weaken.mjs <object.wasm.o> <symbol>... — set WASM_SYM_BINDING_WEAK on the
// named symbols in a wasm object's "linking" symbol table, in place.
//
// Why this exists: rc.h carries pre-ANSI tentative definitions in every TU,
// the wasm backend refuses -fcommon, wasm-ld's --allow-multiple-definition
// keeps the FIRST definition even over a later initialized one, and wasi-sdk's
// llvm-objcopy cannot rewrite wasm symbol tables. Real common-symbol
// semantics — initialized definition beats zero tentative — therefore have to
// be restored by hand: weaken the tentative copies, let the linker's ordinary
// weak-yields-to-strong rule pick the initialized one, in any link order.
// ORing 0x1 into a LEB128 never changes its byte length, so the patch is
// exact and in place.
import { readFileSync, writeFileSync } from "node:fs";

const [file, ...syms] = process.argv.slice(2);
const want = new Set(syms);
const buf = readFileSync(file);

let o = 8; // magic + version
const leb = () => {
  let v = 0, s = 0, b;
  do { b = buf[o++]; v |= (b & 127) << s; s += 7; } while (b & 128);
  return v >>> 0;
};
const name = () => {
  const n = leb(), s = buf.subarray(o, o + n).toString("utf8");
  o += n;
  return s;
};

let patched = 0;
while (o < buf.length) {
  const id = buf[o++], size = leb(), end = o + size;
  if (id === 0) {
    const secname = name();
    if (secname === "linking") {
      leb(); // version
      while (o < end) {
        const sub = buf[o++], subsize = leb(), subend = o + subsize;
        if (sub === 8) { // WASM_SYMBOL_TABLE
          const count = leb();
          for (let i = 0; i < count; i++) {
            const kind = buf[o++];
            const flagsAt = o;
            const flags = leb();
            const defined = !(flags & 0x10); // WASM_SYM_UNDEFINED
            if (kind === 1) { // DATA
              const sym = name();
              if (defined) { leb(); leb(); leb(); } // seg, offset, size
              if (defined && want.has(sym)) { buf[flagsAt] |= 0x01; patched++; }
            } else if (kind === 3) { // SECTION
              leb();
            } else { // FUNCTION/GLOBAL/EVENT/TABLE
              leb(); // index
              if (defined || (flags & 0x40)) {
                const sym = name(); // EXPLICIT_NAME
                if (kind === 0 && defined && want.has(sym)) { buf[flagsAt] |= 0x01; patched++; }
              }
            }
          }
        }
        o = subend;
      }
    }
  }
  o = end;
}
writeFileSync(file, buf);
if (patched !== want.size)
  console.error(`weaken: ${file}: patched ${patched}/${want.size} of: ${syms.join(" ")}`);
