//! SHA-1 and HMAC-SHA1, because `#¤` needs them and the kernel has no
//! dependencies.
//!
//! Plan 9 has the same code in the kernel for the same reason: `devcap.c`
//! calls `hmac_sha1` (`libsec`, linked into the kernel), because a capability
//! is a keyed hash and there is nowhere else for it to be computed. This is
//! RFC 3174 and RFC 2104, written out.
//!
//! **Not a general-purpose facility.** It exists for `#¤` and is not exported
//! as a service to anything else; a userspace program that wants a hash
//! computes its own.

pub const HASHLEN: usize = 20;
const BLOCK: usize = 64;

/// SHA-1 (RFC 3174).
pub fn sha1(data: &[u8]) -> [u8; HASHLEN] {
    let mut h: [u32; 5] = [0x6745_2301, 0xEFCD_AB89, 0x98BA_DCFE, 0x1032_5476, 0xC3D2_E1F0];

    // the padded message: the data, a 0x80 byte, zeroes, then the bit length
    let mut m = data.to_vec();
    let bits = (data.len() as u64) * 8;
    m.push(0x80);
    while m.len() % BLOCK != 56 {
        m.push(0);
    }
    m.extend_from_slice(&bits.to_be_bytes());

    for chunk in m.chunks(BLOCK) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }

        let (mut a, mut b, mut c, mut d, mut e) = (h[0], h[1], h[2], h[3], h[4]);
        for (i, wi) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A82_7999),
                20..=39 => (b ^ c ^ d, 0x6ED9_EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1B_BCDC),
                _ => (b ^ c ^ d, 0xCA62_C1D6),
            };
            let t = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(*wi);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = t;
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
    }

    let mut out = [0u8; HASHLEN];
    for (i, v) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&v.to_be_bytes());
    }
    out
}

/// HMAC-SHA1 (RFC 2104). `hmac_sha1(data, key)` as `devcap.c:231` calls it.
pub fn hmac_sha1(data: &[u8], key: &[u8]) -> [u8; HASHLEN] {
    let mut k = [0u8; BLOCK];
    if key.len() > BLOCK {
        k[..HASHLEN].copy_from_slice(&sha1(key));
    } else {
        k[..key.len()].copy_from_slice(key);
    }

    let mut inner = Vec::with_capacity(BLOCK + data.len());
    inner.extend(k.iter().map(|b| b ^ 0x36));
    inner.extend_from_slice(data);

    let mut outer = Vec::with_capacity(BLOCK + HASHLEN);
    outer.extend(k.iter().map(|b| b ^ 0x5c));
    outer.extend_from_slice(&sha1(&inner));
    sha1(&outer)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    /// RFC 3174's own vectors, plus the empty string.
    #[test]
    fn sha1_matches_the_published_vectors() {
        assert_eq!(hex(&sha1(b"")), "da39a3ee5e6b4b0d3255bfef95601890afd80709");
        assert_eq!(hex(&sha1(b"abc")), "a9993e364706816aba3e25717850c26c9cd0d89d");
        assert_eq!(
            hex(&sha1(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq")),
            "84983e441c3bd26ebaae4aa1f95129e5e54670f1"
        );
        // a million 'a' would be slow here; 1000 blocks of 'a' still exercises
        // the multi-block path
        assert_eq!(sha1(&vec![b'a'; 4096]).len(), HASHLEN);
    }

    /// The padding boundary. 55 bytes is the last that fits with its length
    /// in one block; 56 forces a second; 64 is an exact block. Getting this
    /// wrong is the classic SHA-1 bug and every one of these would still pass
    /// the `abc` vector.
    ///
    /// **Checked against an independent implementation**, not against this
    /// one — an expectation taken from the code it tests proves nothing.
    #[test]
    fn sha1_pads_correctly_at_the_block_boundary() {
        for (n, want) in [
            (55, "c1c8bbdc22796e28c0e15163d20899b65621d65a"),
            (56, "c2db330f6083854c99d4b5bfb6e8f29f201be699"),
            (63, "03f09f5b158a7a8cdad920bddc29b81c18a551f5"),
            (64, "0098ba824b5c16427bd7a1122a5a442a25ec644d"),
            (65, "11655326c708d70319be2610e8a57d9a5b959d3b"),
        ] {
            assert_eq!(hex(&sha1(&vec![b'a'; n])), want, "{n} bytes");
        }
    }

    /// RFC 2104's test vectors.
    #[test]
    fn hmac_sha1_matches_rfc_2104() {
        assert_eq!(
            hex(&hmac_sha1(b"Hi There", &[0x0b; 20])),
            "b617318655057264e28bc0b6fb378c8ef146be00"
        );
        assert_eq!(
            hex(&hmac_sha1(b"what do ya want for nothing?", b"Jefe")),
            "effcdf6ae5eb2fa2d27416d5f184df9c259a7c79"
        );
    }

    /// A key longer than the block is hashed first (RFC 2104 §2), which is
    /// what makes a long key safe rather than truncated.
    #[test]
    fn a_key_longer_than_a_block_is_hashed_first() {
        let long = vec![0xaa; 80];
        assert_eq!(hmac_sha1(b"x", &long), hmac_sha1(b"x", &sha1(&long)));
    }
}
