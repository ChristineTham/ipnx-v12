//! 9P2000 — the wire, and the only place in the system where it exists.
//!
//! 9P is the system's only IPC. Inside the kernel a device presents the file
//! interface as ordinary function calls; wire 9P appears at exactly one
//! boundary, the mount driver, and this module is what that driver speaks.
//! Nothing else may encode or decode a message.
//!
//! 9P2000 and no negotiation beyond it: it is the version `version(5)` defines,
//! and nothing here needs wire compatibility with anything older.

/// Message types. Every request is even, every reply its successor — the
/// property the tag demultiplexer relies on to check a reply belongs to its
/// request.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum T {
    Version = 100,
    Auth = 102,
    Attach = 104,
    Error = 107,
    Flush = 108,
    Walk = 110,
    Open = 112,
    Create = 114,
    Read = 116,
    Write = 118,
    Clunk = 120,
    Remove = 122,
    Stat = 124,
    Wstat = 126,
}

impl T {
    /// The reply that answers this request. `Error` answers anything, so it is
    /// not derived from a request type and has no successor of its own.
    pub fn reply(self) -> u8 {
        self as u8 + 1
    }
}

/// `NOFID`, per attach(5): the fid a walk gives when there is no auth fid.
pub const NOFID: u32 = !0;

/// The smallest message size a mount will negotiate. A `Twrite` may not carry
/// more than `msize - IOHDRSZ` bytes of data, which is why a writer that hands
/// the mount driver more than that must be split by the driver rather than
/// answered short — see `Mount::rdwr`.
pub const IOHDRSZ: usize = 24;

/// A little-endian writer. 9P is little-endian throughout, including the
/// counted strings, which is the one thing worth stating once rather than at
/// every call site.
#[derive(Default)]
pub struct W(Vec<u8>);

impl W {
    pub fn new() -> Self {
        W(Vec::new())
    }
    pub fn u8(mut self, v: u8) -> Self {
        self.0.push(v);
        self
    }
    pub fn u16(mut self, v: u16) -> Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    pub fn u32(mut self, v: u32) -> Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    pub fn u64(mut self, v: u64) -> Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    /// A counted string: a 2-byte length and the bytes, never NUL-terminated.
    pub fn s(self, v: &str) -> Self {
        let b = v.as_bytes();
        self.u16(b.len() as u16).raw(b)
    }
    pub fn raw(mut self, v: &[u8]) -> Self {
        self.0.extend_from_slice(v);
        self
    }
    pub fn into_body(self) -> Vec<u8> {
        self.0
    }

    /// Frame a body as a message: `size[4] type[1] tag[2]` then the body. The
    /// size counts itself, which is the detail every 9P implementation gets
    /// wrong once.
    pub fn frame(self, ty: u8, tag: u16) -> Vec<u8> {
        let body = self.0;
        let size = (4 + 1 + 2 + body.len()) as u32;
        let mut out = Vec::with_capacity(size as usize);
        out.extend_from_slice(&size.to_le_bytes());
        out.push(ty);
        out.extend_from_slice(&tag.to_le_bytes());
        out.extend_from_slice(&body);
        out
    }
}

/// A little-endian reader over a message body. Every accessor is fallible
/// because the bytes come off a wire: a server we did not write can send us
/// anything, and a kernel that trusts a length field is a kernel with a bug.
pub struct R<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> R<'a> {
    pub fn new(b: &'a [u8]) -> Self {
        R { b, i: 0 }
    }
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let s = self.b.get(self.i..self.i + n)?;
        self.i += n;
        Some(s)
    }
    pub fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }
    pub fn u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes(self.take(2)?.try_into().ok()?))
    }
    pub fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
    pub fn u64(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }
    pub fn s(&mut self) -> Option<&'a str> {
        let n = self.u16()? as usize;
        core::str::from_utf8(self.take(n)?).ok()
    }
    pub fn rest(&self) -> &'a [u8] {
        &self.b[self.i..]
    }
}

/// A framed message split into its header and body, or `None` if the buffer
/// does not hold one whole message.
pub struct Msg<'a> {
    pub ty: u8,
    pub tag: u16,
    pub body: &'a [u8],
}

pub fn unframe(buf: &[u8]) -> Option<Msg<'_>> {
    let mut r = R::new(buf);
    let size = r.u32()? as usize;
    if size < 7 || size > buf.len() {
        return None;
    }
    let ty = r.u8()?;
    let tag = r.u16()?;
    Some(Msg { ty, tag, body: &buf[7..size] })
}

/// A qid: the server's name for a file, and the thing a client compares to
/// decide two paths are the same file.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Qid {
    pub qtype: u8,
    pub vers: u32,
    pub path: u64,
}

/// `QTDIR` — the only qid bit the kernel itself reasons about. The rest belong
/// to whoever set them.
pub const QTDIR: u8 = 0x80;

impl Qid {
    pub fn is_dir(self) -> bool {
        self.qtype & QTDIR != 0
    }
    pub fn write(self, w: W) -> W {
        w.u8(self.qtype).u32(self.vers).u64(self.path)
    }
    pub fn read(r: &mut R<'_>) -> Option<Qid> {
        Some(Qid { qtype: r.u8()?, vers: r.u32()?, path: r.u64()? })
    }
}
