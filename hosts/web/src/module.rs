//! **What this machine reads of a module itself**: the memory it imports,
//! and whether it is a WASI program.
//!
//! The kernel's side of the browser machine makes each process's memory —
//! a worker waiting on the kernel cannot be handed one — so it must know the
//! limits the image asks for before any worker sees it. They are in the
//! module's import section (the WebAssembly binary format, §5.5.5), and so
//! is what says a program is WASI's; nothing else of the module is read here.

/// Whether an image is a WASI program: it imports WASI preview 1's calls
/// (docs/architecture.md, *A WASI binary runs natively*). Only preview 1's:
/// it is what `www/wasi.mjs` runs, and the snapshot before it, which the
/// terminal also runs, lays its structures out differently.
pub fn wasi(b: &[u8]) -> bool {
    imports(b).is_some_and(|m| m.iter().any(|m| m == "wasi_snapshot_preview1"))
}

/// The module names of an image's imports, from its import section.
fn imports(b: &[u8]) -> Option<Vec<String>> {
    let mut out = Vec::new();
    walk(b, &mut |module, _| {
        out.push(module.to_string());
        None::<()>
    })?;
    Some(out)
}

/// The memory an image imports: its limits in pages, from the module's
/// import section. `None` if it imports none.
pub fn memimport(b: &[u8]) -> Option<(u32, u32)> {
    let mut found = None;
    walk(b, &mut |_, mem| {
        found = mem;
        mem
    })?;
    found
}

/// Each import of an image, in order: its module's name, and the limits of
/// a memory, if it is one. `f` answering something ends the walk. `None`
/// if the image is not a module this can read.
fn walk<T>(b: &[u8], f: &mut dyn FnMut(&str, Option<(u32, u32)>) -> Option<T>) -> Option<()> {
    fn leb(b: &[u8], at: &mut usize) -> Option<u64> {
        let (mut v, mut shift) = (0u64, 0);
        loop {
            let x = *b.get(*at)?;
            *at += 1;
            v |= ((x & 0x7f) as u64) << shift;
            if x & 0x80 == 0 {
                return Some(v);
            }
            shift += 7;
            if shift > 63 {
                return None;
            }
        }
    }
    fn name<'a>(b: &'a [u8], at: &mut usize) -> Option<&'a str> {
        let n = leb(b, at)? as usize;
        let s = std::str::from_utf8(b.get(*at..at.checked_add(n)?)?).ok()?;
        *at += n;
        Some(s)
    }
    fn limits(b: &[u8], at: &mut usize) -> Option<(u64, Option<u64>)> {
        let flags = *b.get(*at)?;
        *at += 1;
        let min = leb(b, at)?;
        let max = if flags & 1 != 0 { Some(leb(b, at)?) } else { None };
        Some((min, max))
    }
    if b.get(..4)? != b"\0asm" {
        return None;
    }
    let mut at = 8;
    while at < b.len() {
        let id = b[at];
        at += 1;
        let size = leb(b, &mut at)? as usize;
        let end = at.checked_add(size)?;
        if id == 2 {
            let mut p = at;
            for _ in 0..leb(b, &mut p)? {
                let module = name(b, &mut p)?;
                name(b, &mut p)?;
                let kind = *b.get(p)?;
                p += 1;
                let mut mem = None;
                match kind {
                    0 => {
                        leb(b, &mut p)?;
                    }
                    1 => {
                        p += 1;
                        limits(b, &mut p)?;
                    }
                    2 => {
                        let (min, max) = limits(b, &mut p)?;
                        mem = Some((min as u32, max.unwrap_or(65536) as u32));
                    }
                    3 => p += 2,
                    4 => {
                        p += 1;
                        leb(b, &mut p)?;
                    }
                    _ => return None,
                }
                if f(module, mem).is_some() {
                    return Some(());
                }
            }
            return Some(());
        }
        at = end;
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::{memimport, wasi};

    /// The memory import's limits are found among the other imports, and an
    /// image that imports none has none.
    #[test]
    fn the_memory_an_image_imports() {
        // (import "sys" "open" (func)) (import "env" "memory" (memory 2 100 shared))
        let m = [
            0, b'a', b's', b'm', 1, 0, 0, 0, //
            1, 4, 1, 0x60, 0, 0, // type section: one () -> ()
            2, 27, 2, // import section, two imports
            3, b's', b'y', b's', 4, b'o', b'p', b'e', b'n', 0, 0, //
            3, b'e', b'n', b'v', 6, b'm', b'e', b'm', b'o', b'r', b'y', 2, 3, 2, 100,
        ];
        assert_eq!(memimport(&m), Some((2, 100)));
        assert_eq!(memimport(&m[..14]), None);
        assert_eq!(memimport(b"junk"), None);
    }

    /// A program that imports WASI preview 1's calls is one, whatever else
    /// it imports; a program of this system's, or the snapshot before
    /// preview 1, is not.
    #[test]
    fn a_wasi_program_is_known_by_its_imports() {
        let image = |module: &[u8]| {
            // (type (func)) (import <module> "fd_write" (func (type 0)))
            let mut imp = vec![1, module.len() as u8];
            imp.extend_from_slice(module);
            imp.extend_from_slice(&[8, b'f', b'd', b'_', b'w', b'r', b'i', b't', b'e', 0, 0]);
            let mut m = vec![0, b'a', b's', b'm', 1, 0, 0, 0, 1, 4, 1, 0x60, 0, 0, 2, imp.len() as u8];
            m.extend_from_slice(&imp);
            m
        };
        assert!(wasi(&image(b"wasi_snapshot_preview1")));
        assert!(!wasi(&image(b"wasi_unstable")));
        assert!(!wasi(&image(b"sys")));
        assert!(!wasi(b"junk"));
        assert_eq!(memimport(&image(b"wasi_snapshot_preview1")), None);
    }
}
