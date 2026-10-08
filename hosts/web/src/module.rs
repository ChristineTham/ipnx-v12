//! **What this machine reads of a module itself**: the memory it imports.
//!
//! The kernel's side of the browser machine makes each process's memory —
//! a worker waiting on the kernel cannot be handed one — so it must know the
//! limits the image asks for before any worker sees it. They are in the
//! module's import section (the WebAssembly binary format, §5.5.5), and
//! nothing else of the module is read here.

/// The memory an image imports: its limits in pages, from the module's
/// import section. `None` if it imports none.
pub fn memimport(b: &[u8]) -> Option<(u32, u32)> {
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
    fn name(b: &[u8], at: &mut usize) -> Option<()> {
        let n = leb(b, at)? as usize;
        *at = at.checked_add(n)?;
        Some(())
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
                name(b, &mut p)?;
                name(b, &mut p)?;
                let kind = *b.get(p)?;
                p += 1;
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
                        return Some((min as u32, max.unwrap_or(65536) as u32));
                    }
                    3 => p += 2,
                    4 => {
                        p += 1;
                        leb(b, &mut p)?;
                    }
                    _ => return None,
                }
            }
            return None;
        }
        at = end;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::memimport;

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
}
