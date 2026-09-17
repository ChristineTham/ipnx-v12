//! CONTRACT: 9P2000 is the only IPC, and the wire exists at exactly one
//! boundary.
//!
//! The codec is held to the protocol, not to our convenience — and to the one
//! property that matters for safety: a server we did not write is on the other
//! end, so every decode must be able to fail without taking the kernel down.

use ipnx_kernel::ninep::{unframe, Qid, R, T, W, QTDIR};

#[test]
fn a_frame_counts_its_own_size() {
    let m = W::new().u32(8192).s("9P2000").frame(T::Version as u8, 1);
    assert_eq!(u32::from_le_bytes(m[..4].try_into().unwrap()) as usize, m.len());
}

#[test]
fn a_reply_is_its_request_plus_one() {
    assert_eq!(T::Walk.reply(), 111);
    assert_eq!(T::Read.reply(), 117);
    assert_eq!(T::Write.reply(), 119);
}

#[test]
fn strings_are_counted_and_never_terminated() {
    assert_eq!(W::new().s("motd").into_body(), vec![4, 0, b'm', b'o', b't', b'd']);
}

#[test]
fn a_message_round_trips() {
    let m = W::new().u32(4).u64(9).s("hello").frame(T::Write as u8, 7);
    let msg = unframe(&m).expect("one whole message");
    assert_eq!((msg.ty, msg.tag), (T::Write as u8, 7));
    let mut r = R::new(msg.body);
    assert_eq!((r.u32(), r.u64(), r.s()), (Some(4), Some(9), Some("hello")));
}

#[test]
fn a_hostile_message_cannot_panic_the_kernel() {
    // A length that lies, and a frame that claims more than it holds. Both
    // must answer None rather than read past the buffer.
    let mut lying = vec![200u8, 0];
    lying.extend_from_slice(b"short");
    assert_eq!(R::new(&lying).s(), None);

    let m = W::new().u32(4).frame(T::Read as u8, 1);
    assert!(unframe(&m[..m.len() - 1]).is_none());
}

#[test]
fn a_qid_round_trips_and_knows_a_directory() {
    let q = Qid { qtype: QTDIR, vers: 3, path: 42 };
    let body = q.write(W::new()).into_body();
    let got = Qid::read(&mut R::new(&body)).unwrap();
    assert_eq!(got, q);
    assert!(got.is_dir());
}
