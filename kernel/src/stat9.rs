// 9P2000 stat(5) marshalling: size[2] type[2] dev[4] qid.type[1] qid.vers[4]
// qid.path[8] mode[4] atime[4] mtime[4] length[8] name[s] uid[s] gid[s] muid[s]
// — the Rust port of supervisor/stat9.mjs, byte-identical output.

pub const QTDIR: u8 = 0x80;
pub const QTFILE: u8 = 0x00;
pub const DMDIR: u32 = 0x8000_0000;
pub const DMSETUID: u32 = 0x0008_0000;

pub struct StatIn<'a> {
    pub name: &'a str,
    pub qtype: u8,
    pub qpath: u64,
    pub qvers: u32,
    pub mode: u32,
    pub atime: u32,
    pub mtime: u32,
    pub length: u64,
    pub uid: &'a str,
    pub gid: &'a str,
    pub muid: &'a str,
}

impl<'a> Default for StatIn<'a> {
    fn default() -> Self {
        StatIn { name: "", qtype: 0, qpath: 0, qvers: 0, mode: 0, atime: 0,
                 mtime: 0, length: 0, uid: "kitty", gid: "kitty", muid: "kitty" }
    }
}

pub fn marshal_stat(d: &StatIn) -> Vec<u8> {
    let s = |out: &mut Vec<u8>, x: &str| {
        out.extend_from_slice(&(x.len() as u16).to_le_bytes());
        out.extend_from_slice(x.as_bytes());
    };
    let mut out = Vec::with_capacity(64);
    out.extend_from_slice(&[0, 0]); // size, filled last
    out.extend_from_slice(&0u16.to_le_bytes()); // type
    out.extend_from_slice(&0u32.to_le_bytes()); // dev
    out.push(d.qtype);
    out.extend_from_slice(&d.qvers.to_le_bytes());
    out.extend_from_slice(&d.qpath.to_le_bytes());
    out.extend_from_slice(&d.mode.to_le_bytes());
    out.extend_from_slice(&d.atime.to_le_bytes());
    out.extend_from_slice(&d.mtime.to_le_bytes());
    out.extend_from_slice(&d.length.to_le_bytes());
    s(&mut out, d.name);
    s(&mut out, d.uid);
    s(&mut out, d.gid);
    s(&mut out, d.muid);
    let size = (out.len() - 2) as u16; // size excludes itself
    out[0..2].copy_from_slice(&size.to_le_bytes());
    out
}

pub struct StatOut {
    pub mode: u32,
    pub atime: u32,
    pub mtime: u32,
    pub length: u64,
    pub name: String,
    pub uid: String,
    pub gid: String,
}

// Read one stat(5) record. For wstat, "don't touch" is ~0 for integers and
// the zero-length string for names, per stat(5) — the caller checks.
pub fn parse_stat(b: &[u8]) -> Option<StatOut> {
    let u16at = |o: usize| -> Option<usize> {
        Some(u16::from_le_bytes(b.get(o..o + 2)?.try_into().ok()?) as usize)
    };
    let str_at = |o: usize| -> Option<(String, usize)> {
        let n: usize = u16::from_le_bytes(b.get(o..o + 2)?.try_into().ok()?) as usize;
        let s = String::from_utf8_lossy(b.get(o + 2..o + 2 + n)?).into_owned();
        Some((s, o + 2 + n))
    };
    let _sz: usize = u16at(0)?;
    let mode = u32::from_le_bytes(b.get(21..25)?.try_into().ok()?);
    let atime = u32::from_le_bytes(b.get(25..29)?.try_into().ok()?);
    let mtime = u32::from_le_bytes(b.get(29..33)?.try_into().ok()?);
    let length = u64::from_le_bytes(b.get(33..41)?.try_into().ok()?);
    let (name, o) = str_at(41)?;
    let (uid, o) = str_at(o)?;
    let (gid, _) = str_at(o)?;
    Some(StatOut { mode, atime, mtime, length, name, uid, gid })
}
