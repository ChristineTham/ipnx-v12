# zlib, decompression-only, in pure Python — a personality file for ipnx.
#
# This CPython wasi build carries no zlib C module (measured 2026-08-29:
# absent from sys.builtin_module_names). Wheels, zips and zlib streams are
# DEFLATE, so the personality supplies the algorithm in Python: an inflate
# in the shape of Mark Adler's puff.c (the RFC 1951 reference decoder),
# crc32 delegated to the binascii builtin, adler32 implemented directly.
# Compression is honestly absent: compressobj/compress raise. When a build
# with the real zlib lands, this file retires unchanged in spirit — the
# porting inversion works at the stdlib layer too.

from binascii import crc32 as crc32  # noqa: F401  (re-exported)

MAX_WBITS = 15
DEFLATED = 8

MAXBITS = 15
MAXLCODES = 286
MAXDCODES = 30

_LENS = (3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31,
         35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258)
_LEXT = (0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2,
         3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0)
_DISTS = (1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193,
          257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145,
          8193, 12289, 16385, 24577)
_DEXT = (0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6,
         7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13)
_ORDER = (16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15)


class error(Exception):
    pass


class _State:
    __slots__ = ("data", "pos", "bitbuf", "bitcnt", "out")

    def __init__(self, data):
        self.data = data
        self.pos = 0
        self.bitbuf = 0
        self.bitcnt = 0
        self.out = bytearray()

    def bits(self, need):
        val = self.bitbuf
        while self.bitcnt < need:
            if self.pos >= len(self.data):
                raise error("incomplete deflate stream")
            val |= self.data[self.pos] << self.bitcnt
            self.pos += 1
            self.bitcnt += 8
        self.bitbuf = val >> need
        self.bitcnt -= need
        return val & ((1 << need) - 1)


class _Huffman:
    __slots__ = ("count", "symbol")

    def __init__(self, lengths, n):
        self.count = [0] * (MAXBITS + 1)
        for i in range(n):
            self.count[lengths[i]] += 1
        offs = [0] * (MAXBITS + 1)
        for l in range(1, MAXBITS):
            offs[l + 1] = offs[l] + self.count[l]
        self.symbol = [0] * n
        for i in range(n):
            if lengths[i]:
                self.symbol[offs[lengths[i]]] = i
                offs[lengths[i]] += 1


def _decode(s, h):
    code = first = index = 0
    for l in range(1, MAXBITS + 1):
        code |= s.bits(1)
        count = h.count[l]
        if code - count < first:
            return h.symbol[index + (code - first)]
        index += count
        first += count
        first <<= 1
        code <<= 1
    raise error("invalid huffman code")


def _codes(s, lencode, distcode):
    out = s.out
    while True:
        sym = _decode(s, lencode)
        if sym < 256:
            out.append(sym)
        elif sym == 256:
            return
        else:
            sym -= 257
            if sym >= 29:
                raise error("invalid length code")
            length = _LENS[sym] + s.bits(_LEXT[sym])
            sym = _decode(s, distcode)
            dist = _DISTS[sym] + s.bits(_DEXT[sym])
            if dist > len(out):
                raise error("distance too far back")
            if dist >= length:
                out += out[-dist:len(out) - dist + length]
            else:
                for _ in range(length):
                    out.append(out[-dist])


def _stored(s):
    s.bitbuf = s.bitcnt = 0
    if s.pos + 4 > len(s.data):
        raise error("incomplete stored block")
    n = s.data[s.pos] | (s.data[s.pos + 1] << 8)
    s.pos += 4
    if s.pos + n > len(s.data):
        raise error("stored block overruns input")
    s.out += s.data[s.pos:s.pos + n]
    s.pos += n


_fixed_cache = None


def _fixed():
    global _fixed_cache
    if _fixed_cache is None:
        lens = [8] * 144 + [9] * 112 + [7] * 24 + [8] * 8
        _fixed_cache = (_Huffman(lens, 288), _Huffman([5] * MAXDCODES, MAXDCODES))
    return _fixed_cache


def _dynamic(s):
    nlen = s.bits(5) + 257
    ndist = s.bits(5) + 1
    ncode = s.bits(4) + 4
    if nlen > MAXLCODES or ndist > MAXDCODES:
        raise error("too many codes")
    lengths = [0] * (MAXLCODES + MAXDCODES)
    for i in range(ncode):
        lengths[_ORDER[i]] = s.bits(3)
    lencode = _Huffman(lengths, 19)
    lengths = [0] * (MAXLCODES + MAXDCODES)
    i = 0
    while i < nlen + ndist:
        sym = _decode(s, lencode)
        if sym < 16:
            lengths[i] = sym
            i += 1
        else:
            if sym == 16:
                if i == 0:
                    raise error("repeat with no first length")
                rep, val = 3 + s.bits(2), lengths[i - 1]
            elif sym == 17:
                rep, val = 3 + s.bits(3), 0
            else:
                rep, val = 11 + s.bits(7), 0
            if i + rep > nlen + ndist:
                raise error("too many lengths")
            for _ in range(rep):
                lengths[i] = val
                i += 1
    if lengths[256] == 0:
        raise error("no end-of-block code")
    return _Huffman(lengths, nlen), _Huffman(lengths[nlen:], ndist)


def _inflate(data):
    s = _State(data)
    while True:
        last = s.bits(1)
        kind = s.bits(2)
        if kind == 0:
            _stored(s)
        elif kind == 1:
            _codes(s, *_fixed())
        elif kind == 2:
            _codes(s, *_dynamic(s))
        else:
            raise error("invalid block type")
        if last:
            return bytes(s.out), s.pos + (0 if s.bitcnt < 8 else 0)


def adler32(data, value=1):
    a, b = value & 0xffff, (value >> 16) & 0xffff
    for i in range(0, len(data), 5552):
        for byte in data[i:i + 5552]:
            a += byte
            b += a
        a %= 65521
        b %= 65521
    return (b << 16) | a


def decompress(data, wbits=MAX_WBITS, bufsize=16384):
    data = bytes(data)
    if wbits < 0:
        return _inflate(data)[0]
    if wbits >= 8:
        if len(data) < 2 or (data[0] & 0x0f) != DEFLATED or ((data[0] << 8) | data[1]) % 31:
            raise error("bad zlib header")
        if data[1] & 0x20:
            raise error("preset dictionaries unsupported")
        out, _ = _inflate(data[2:])
        return out
    raise error(f"unsupported wbits {wbits}")


class _Decompressor:
    """Buffer-everything decompressobj: correct for consumers that feed the
    whole stream (this personality's zipfile use goes through pip's own
    extractor instead). Streaming consumers get their bytes at flush."""

    def __init__(self, wbits):
        self._wbits = wbits
        self._in = bytearray()
        self.eof = False
        self.unconsumed_tail = b""
        self.unused_data = b""

    def decompress(self, data, max_length=0):
        self._in += data
        try:
            out = decompress(bytes(self._in), self._wbits)
        except error:
            return b""
        self._in.clear()
        self.eof = True
        return out

    def flush(self, length=None):
        if not self._in:
            return b""
        out = decompress(bytes(self._in), self._wbits)
        self._in.clear()
        self.eof = True
        return out


def decompressobj(wbits=MAX_WBITS, zdict=None):
    return _Decompressor(wbits)


def compress(data, level=-1):
    raise error("compression is not available in this personality (inflate only)")


def compressobj(*a, **k):
    raise error("compression is not available in this personality (inflate only)")
