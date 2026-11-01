//! gzip (RFC 1952) with a small deflate encoder and a complete inflate decoder, standard library only.
//! The encoder uses LZ77 with fixed Huffman codes and falls back to stored blocks for data that does
//! not compress (pictures), so a backup is never much bigger than its contents.

const fn make_table() -> [u32; 256] {
    let mut t = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            k += 1;
        }
        t[i] = c;
        i += 1;
    }
    t
}
static CRC_TABLE: [u32; 256] = make_table();

pub fn crc32(data: &[u8]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for &b in data {
        c = CRC_TABLE[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8);
    }
    !c
}

const LEN_BASE: [u16; 29] = [3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258];
const LEN_EXTRA: [u8; 29] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
const DIST_BASE: [u16; 30] = [1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577];
const DIST_EXTRA: [u8; 30] = [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13];

struct Bits {
    out: Vec<u8>,
    acc: u64,
    n: u32,
}

impl Bits {
    fn put(&mut self, v: u32, bits: u32) {
        self.acc |= (v as u64) << self.n;
        self.n += bits;
        while self.n >= 8 {
            self.out.push(self.acc as u8);
            self.acc >>= 8;
            self.n -= 8;
        }
    }
    /// Huffman codes go out most significant bit first.
    fn put_code(&mut self, code: u32, bits: u32) {
        let mut r = 0u32;
        for i in 0..bits {
            if code & (1 << i) != 0 {
                r |= 1 << (bits - 1 - i);
            }
        }
        self.put(r, bits);
    }
    fn align(&mut self) {
        if self.n > 0 {
            self.out.push(self.acc as u8);
            self.acc = 0;
            self.n = 0;
        }
    }
}

fn put_lit(b: &mut Bits, sym: u32) {
    match sym {
        0..=143 => b.put_code(0x30 + sym, 8),
        144..=255 => b.put_code(0x190 + (sym - 144), 9),
        256..=279 => b.put_code(sym - 256, 7),
        _ => b.put_code(0xC0 + (sym - 280), 8),
    }
}

fn lit_bits(sym: u32) -> u64 {
    match sym {
        0..=143 => 8,
        144..=255 => 9,
        256..=279 => 7,
        _ => 8,
    }
}

/// One LZ77 token: a literal byte, or a (length, distance) pair.
#[derive(Clone, Copy)]
enum Tok {
    Lit(u8),
    Match(u16, u16),
}

const CHUNK: usize = 32768;
const NONE: u32 = u32::MAX;

fn hash3(d: &[u8], i: usize) -> usize {
    (((d[i] as usize) << 10) ^ ((d[i + 1] as usize) << 5) ^ (d[i + 2] as usize)) & 0x7FFF
}

pub fn deflate(data: &[u8]) -> Vec<u8> {
    let mut b = Bits { out: Vec::with_capacity(data.len() / 2 + 16), acc: 0, n: 0 };
    if data.is_empty() {
        b.put(1, 1);
        b.put(0, 2);
        b.align();
        b.out.extend_from_slice(&[0, 0, 0xFF, 0xFF]);
        return b.out;
    }
    let mut head = vec![NONE; 1 << 15];
    let mut prev = vec![NONE; 1 << 15];
    let mut start = 0usize;
    while start < data.len() {
        let end = (start + CHUNK).min(data.len());
        let last = end == data.len();
        let mut toks: Vec<Tok> = Vec::with_capacity(end - start);
        let mut cost: u64 = 3 + 7;
        let mut i = start;
        while i < end {
            let mut best_len = 0usize;
            let mut best_dist = 0usize;
            if i + 3 <= data.len() {
                let h = hash3(data, i);
                let mut cand = head[h];
                let mut depth = 48;
                let max = (end - i).min(258);
                while cand != NONE && depth > 0 {
                    let c = cand as usize;
                    if c >= i || i - c >= CHUNK {
                        break;
                    }
                    let mut l = 0;
                    while l < max && data[c + l] == data[i + l] {
                        l += 1;
                    }
                    if l > best_len {
                        best_len = l;
                        best_dist = i - c;
                        if l == max {
                            break;
                        }
                    }
                    let next = prev[c & 0x7FFF];
                    if next != NONE && next as usize >= c {
                        break;
                    }
                    cand = next;
                    depth -= 1;
                }
            }
            let step;
            if best_len >= 3 {
                let mut lc = 28;
                while LEN_BASE[lc] as usize > best_len {
                    lc -= 1;
                }
                let mut dc = 29;
                while DIST_BASE[dc] as usize > best_dist {
                    dc -= 1;
                }
                cost += lit_bits(257 + lc as u32) + LEN_EXTRA[lc] as u64 + 5 + DIST_EXTRA[dc] as u64;
                toks.push(Tok::Match(best_len as u16, best_dist as u16));
                step = best_len;
            } else {
                cost += lit_bits(data[i] as u32);
                toks.push(Tok::Lit(data[i]));
                step = 1;
            }
            for k in i..i + step {
                if k + 3 <= data.len() {
                    let h = hash3(data, k);
                    prev[k & 0x7FFF] = head[h];
                    head[h] = k as u32;
                }
            }
            i += step;
        }
        let stored_cost = 3 + 7 + 32 + 8 * (end - start) as u64;
        if cost <= stored_cost {
            b.put(last as u32, 1);
            b.put(1, 2);
            for t in &toks {
                match *t {
                    Tok::Lit(c) => put_lit(&mut b, c as u32),
                    Tok::Match(len, dist) => {
                        let (len, dist) = (len as usize, dist as usize);
                        let mut lc = 28;
                        while LEN_BASE[lc] as usize > len {
                            lc -= 1;
                        }
                        put_lit(&mut b, 257 + lc as u32);
                        b.put((len - LEN_BASE[lc] as usize) as u32, LEN_EXTRA[lc] as u32);
                        let mut dc = 29;
                        while DIST_BASE[dc] as usize > dist {
                            dc -= 1;
                        }
                        b.put_code(dc as u32, 5);
                        b.put((dist - DIST_BASE[dc] as usize) as u32, DIST_EXTRA[dc] as u32);
                    }
                }
            }
            put_lit(&mut b, 256);
        } else {
            b.put(last as u32, 1);
            b.put(0, 2);
            b.align();
            let n = (end - start) as u16;
            b.out.extend_from_slice(&n.to_le_bytes());
            b.out.extend_from_slice(&(!n).to_le_bytes());
            b.out.extend_from_slice(&data[start..end]);
        }
        start = end;
    }
    b.align();
    b.out
}

struct Reader<'a> {
    d: &'a [u8],
    pos: usize,
    acc: u32,
    n: u32,
}

impl Reader<'_> {
    fn bits(&mut self, k: u32) -> Result<u32, String> {
        while self.n < k {
            if self.pos >= self.d.len() {
                return Err("The compressed data ends too early.".to_string());
            }
            self.acc |= (self.d[self.pos] as u32) << self.n;
            self.pos += 1;
            self.n += 8;
        }
        let v = if k == 0 { 0 } else { self.acc & ((1u32 << k) - 1) };
        self.acc >>= k;
        self.n -= k;
        Ok(v)
    }
}

struct Huff {
    count: [u16; 16],
    symbol: Vec<u16>,
}

fn build(lengths: &[u8]) -> Huff {
    let mut count = [0u16; 16];
    for &l in lengths {
        count[l as usize] += 1;
    }
    count[0] = 0;
    let mut offs = [0u16; 16];
    for i in 1..15 {
        offs[i + 1] = offs[i] + count[i];
    }
    let mut symbol = vec![0u16; lengths.len()];
    for (s, &l) in lengths.iter().enumerate() {
        if l != 0 {
            symbol[offs[l as usize] as usize] = s as u16;
            offs[l as usize] += 1;
        }
    }
    Huff { count, symbol }
}

fn decode(r: &mut Reader, h: &Huff) -> Result<u16, String> {
    let (mut code, mut first, mut index) = (0i32, 0i32, 0i32);
    for len in 1..16 {
        code |= r.bits(1)? as i32;
        let count = h.count[len] as i32;
        if code - count < first {
            return h.symbol.get((index + (code - first)) as usize).copied().ok_or_else(|| "Bad compressed data.".to_string());
        }
        index += count;
        first += count;
        first <<= 1;
        code <<= 1;
    }
    Err("Bad compressed data.".to_string())
}

fn inflate_codes(r: &mut Reader, out: &mut Vec<u8>, lit: &Huff, dist: &Huff) -> Result<(), String> {
    loop {
        let sym = decode(r, lit)? as usize;
        if sym < 256 {
            out.push(sym as u8);
        } else if sym == 256 {
            return Ok(());
        } else {
            let i = sym - 257;
            if i >= 29 {
                return Err("Bad compressed data.".to_string());
            }
            let len = LEN_BASE[i] as usize + r.bits(LEN_EXTRA[i] as u32)? as usize;
            let ds = decode(r, dist)? as usize;
            if ds >= 30 {
                return Err("Bad compressed data.".to_string());
            }
            let d = DIST_BASE[ds] as usize + r.bits(DIST_EXTRA[ds] as u32)? as usize;
            if d > out.len() {
                return Err("Bad compressed data.".to_string());
            }
            for _ in 0..len {
                let c = out[out.len() - d];
                out.push(c);
            }
        }
    }
}

pub fn inflate(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut r = Reader { d: data, pos: 0, acc: 0, n: 0 };
    let mut out: Vec<u8> = Vec::with_capacity(data.len() * 2);
    loop {
        let last = r.bits(1)? == 1;
        match r.bits(2)? {
            0 => {
                r.acc = 0;
                r.n = 0;
                if r.pos + 4 > data.len() {
                    return Err("The compressed data ends too early.".to_string());
                }
                let len = u16::from_le_bytes([data[r.pos], data[r.pos + 1]]) as usize;
                r.pos += 4;
                if r.pos + len > data.len() {
                    return Err("The compressed data ends too early.".to_string());
                }
                out.extend_from_slice(&data[r.pos..r.pos + len]);
                r.pos += len;
            }
            1 => {
                let mut l = [0u8; 288];
                for (i, v) in l.iter_mut().enumerate() {
                    *v = match i {
                        0..=143 => 8,
                        144..=255 => 9,
                        256..=279 => 7,
                        _ => 8,
                    };
                }
                inflate_codes(&mut r, &mut out, &build(&l), &build(&[5u8; 30]))?;
            }
            2 => {
                let nlen = r.bits(5)? as usize + 257;
                let ndist = r.bits(5)? as usize + 1;
                let ncode = r.bits(4)? as usize + 4;
                const ORDER: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];
                let mut cl = [0u8; 19];
                for &o in ORDER.iter().take(ncode) {
                    cl[o] = r.bits(3)? as u8;
                }
                let ch = build(&cl);
                let mut lengths = vec![0u8; nlen + ndist];
                let mut i = 0;
                while i < nlen + ndist {
                    let sym = decode(&mut r, &ch)?;
                    if sym < 16 {
                        lengths[i] = sym as u8;
                        i += 1;
                    } else {
                        let (val, rep) = match sym {
                            16 => {
                                if i == 0 {
                                    return Err("Bad compressed data.".to_string());
                                }
                                (lengths[i - 1], 3 + r.bits(2)? as usize)
                            }
                            17 => (0, 3 + r.bits(3)? as usize),
                            _ => (0, 11 + r.bits(7)? as usize),
                        };
                        if i + rep > nlen + ndist {
                            return Err("Bad compressed data.".to_string());
                        }
                        for _ in 0..rep {
                            lengths[i] = val;
                            i += 1;
                        }
                    }
                }
                inflate_codes(&mut r, &mut out, &build(&lengths[..nlen]), &build(&lengths[nlen..]))?;
            }
            _ => return Err("Bad compressed data.".to_string()),
        }
        if last {
            return Ok(out);
        }
    }
}

pub fn gzip(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x1F, 0x8B, 8, 0, 0, 0, 0, 0, 0, 0xFF];
    out.extend(deflate(data));
    out.extend_from_slice(&crc32(data).to_le_bytes());
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out
}

pub fn gunzip(data: &[u8]) -> Result<Vec<u8>, String> {
    if data.len() < 18 || data[0] != 0x1F || data[1] != 0x8B || data[2] != 8 {
        return Err("This is not a gzip file.".to_string());
    }
    let flags = data[3];
    let mut p = 10;
    if flags & 4 != 0 {
        if p + 2 > data.len() {
            return Err("Bad gzip header.".to_string());
        }
        p += 2 + u16::from_le_bytes([data[p], data[p + 1]]) as usize;
    }
    for bit in [8u8, 16] {
        if flags & bit != 0 {
            while p < data.len() && data[p] != 0 {
                p += 1;
            }
            p += 1;
        }
    }
    if flags & 2 != 0 {
        p += 2;
    }
    if p >= data.len() {
        return Err("Bad gzip header.".to_string());
    }
    let out = inflate(&data[p..data.len() - 8])?;
    let t = &data[data.len() - 8..];
    let want = u32::from_le_bytes([t[0], t[1], t[2], t[3]]);
    if crc32(&out) != want {
        return Err("The file is damaged (checksum does not match).".to_string());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noise(n: usize) -> Vec<u8> {
        let mut x = 12345u32;
        (0..n)
            .map(|_| {
                x = x.wrapping_mul(1664525).wrapping_add(1013904223);
                (x >> 24) as u8
            })
            .collect()
    }

    #[test]
    fn round_trips() {
        let text: Vec<u8> = "the quick brown fox jumps over the lazy dog. ".repeat(3000).into_bytes();
        for data in [vec![], b"a".to_vec(), text.clone(), noise(100_000), [noise(5000), text].concat()] {
            assert_eq!(gunzip(&gzip(&data)).unwrap(), data);
        }
    }

    #[test]
    fn repetitive_data_shrinks_and_noise_barely_grows() {
        let text: Vec<u8> = "abcabcabc hello hello hello ".repeat(5000).into_bytes();
        assert!(gzip(&text).len() < text.len() / 5);
        let n = noise(200_000);
        assert!(gzip(&n).len() < n.len() + n.len() / 100 + 64);
    }

    #[test]
    fn crc_matches_the_known_value() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn reads_output_of_the_gzip_tool() {
        let g: [u8; 308] = [0x1f, 0x8b, 0x8, 0x0, 0xb8, 0xaf, 0xcb, 0x6a, 0x2, 0xff, 0xcb, 0x48, 0xcd, 0xc9, 0xc9, 0x57, 0xc8, 0x40, 0x27, 0xb9, 0x32, 0x48, 0x14, 0x67, 0x60, 0x64, 0x62, 0x66, 0x61, 0x65, 0x63, 0xe7, 0xe0, 0xe4, 0xe2, 0xe6, 0xe1, 0xe5, 0xe3, 0x17, 0x10, 0x14, 0x12, 0x16, 0x11, 0x15, 0x13, 0x97, 0x90, 0x94, 0x92, 0x96, 0x91, 0x95, 0x93, 0x57, 0x50, 0x54, 0x52, 0x56, 0x51, 0x55, 0x53, 0xd7, 0xd0, 0xd4, 0xd2, 0xd6, 0xd1, 0xd5, 0xd3, 0x37, 0x30, 0x34, 0x32, 0x36, 0x31, 0x35, 0x33, 0xb7, 0xb0, 0xb4, 0xb2, 0xb6, 0xb1, 0xb5, 0xb3, 0x77, 0x70, 0x74, 0x72, 0x76, 0x71, 0x75, 0x73, 0xf7, 0xf0, 0xf4, 0xf2, 0xf6, 0xf1, 0xf5, 0xf3, 0xf, 0x8, 0xc, 0xa, 0xe, 0x9, 0xd, 0xb, 0x8f, 0x88, 0x8c, 0x8a, 0x8e, 0x89, 0x8d, 0x8b, 0x4f, 0x48, 0x4c, 0x4a, 0x4e, 0x49, 0x4d, 0x4b, 0xcf, 0xc8, 0xcc, 0xca, 0xce, 0xc9, 0xcd, 0xcb, 0x2f, 0x28, 0x2c, 0x2a, 0x2e, 0x29, 0x2d, 0x2b, 0xaf, 0xa8, 0xac, 0xaa, 0xae, 0xa9, 0xad, 0xab, 0x6f, 0x68, 0x6c, 0x6a, 0x6e, 0x69, 0x6d, 0x6b, 0xef, 0xe8, 0xec, 0xea, 0xee, 0xe9, 0xed, 0xeb, 0x9f, 0x30, 0x71, 0xd2, 0xe4, 0x29, 0x53, 0xa7, 0x4d, 0x9f, 0x31, 0x73, 0xd6, 0xec, 0x39, 0x73, 0xe7, 0xcd, 0x5f, 0xb0, 0x70, 0xd1, 0xe2, 0x25, 0x4b, 0x97, 0x2d, 0x5f, 0xb1, 0x72, 0xd5, 0xea, 0x35, 0x6b, 0xd7, 0xad, 0xdf, 0xb0, 0x71, 0xd3, 0xe6, 0x2d, 0x5b, 0xb7, 0x6d, 0xdf, 0xb1, 0x73, 0xd7, 0xee, 0x3d, 0x7b, 0xf7, 0xed, 0x3f, 0x70, 0xf0, 0xd0, 0xe1, 0x23, 0x47, 0x8f, 0x1d, 0x3f, 0x71, 0xf2, 0xd4, 0xe9, 0x33, 0x67, 0xcf, 0x9d, 0xbf, 0x70, 0xf1, 0xd2, 0xe5, 0x2b, 0x57, 0xaf, 0x5d, 0xbf, 0x71, 0xf3, 0xd6, 0xed, 0x3b, 0x77, 0xef, 0xdd, 0x7f, 0xf0, 0xf0, 0xd1, 0xe3, 0x27, 0x4f, 0x9f, 0x3d, 0x7f, 0xf1, 0xf2, 0xd5, 0xeb, 0x37, 0x6f, 0xdf, 0xbd, 0xff, 0xf0, 0xf1, 0xd3, 0xe7, 0x2f, 0x5f, 0xbf, 0x7d, 0xff, 0xf1, 0xf3, 0xd7, 0xef, 0x3f, 0x7f, 0xff, 0xfd, 0x1f, 0xf5, 0xff, 0xc8, 0xf6, 0x3f, 0x0, 0x30, 0x70, 0x3f, 0xaf, 0x48, 0x3, 0x0, 0x0];
        let want: Vec<u8> = "hello hello hello hello\n".repeat(3).into_bytes().into_iter().chain((0..=255u8).cycle().take(768)).collect();
        assert_eq!(gunzip(&g).unwrap(), want);
    }

    #[test]
    fn damaged_data_is_reported() {
        let mut g = gzip(b"some data that is long enough to matter, some data that is long enough");
        let n = g.len();
        g[n - 6] ^= 0xFF;
        assert!(gunzip(&g).is_err());
        assert!(gunzip(b"not gzip at all, really not").is_err());
    }
}
