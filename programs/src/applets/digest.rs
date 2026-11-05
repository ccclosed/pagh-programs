//! Hash primitives shared by `md5sum`, `sha1sum`, `sha224sum`, `sha256sum`,
//! `sha384sum`, `sha512sum` and `b2sum`.
//!
//! The module is deliberately free of `libpagh` types: the applets read a file
//! and call [`hash`], and the same file can be compiled by a host `rustc` for
//! known-vector tests. Everything works on byte slices and fixed-size arrays;
//! no input can make it panic (there are no unchecked slices indexed by
//! user-controlled lengths).
//!
//! Implemented algorithms:
//!
//! * MD5 (RFC 1321);
//! * SHA-1 (FIPS 180-4);
//! * SHA-224 and SHA-256, SHA-384 and SHA-512 (FIPS 180-4). SHA-224 is
//!   SHA-256 with the SHA-224 IV and a 28-byte truncation; SHA-384 is SHA-512
//!   with the SHA-384 IV and a 48-byte truncation. The compression function is
//!   shared, as the standard defines it;
//! * BLAKE2b-512, unkeyed (RFC 7693).
//!
//! Each is a one-shot implementation: the caller supplies the whole message.
//! That is the same memory policy as `sort`, `diff` and the other whole-file
//! applets in this bundle.

/// The digest algorithms exposed by the seven `*sum` applets.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Algorithm {
    Md5,
    Sha1,
    Sha224,
    Sha256,
    Sha384,
    Sha512,
    Blake2b,
}

impl Algorithm {
    /// The digest length in bytes (also the number of hex digits divided by 2).
    pub const fn length(self) -> usize {
        match self {
            Algorithm::Md5 => 16,
            Algorithm::Sha1 => 20,
            Algorithm::Sha224 => 28,
            Algorithm::Sha256 => 32,
            Algorithm::Sha384 => 48,
            Algorithm::Sha512 => 64,
            Algorithm::Blake2b => 64,
        }
    }

    /// The usual program name; useful to callers that build diagnostics.
    #[allow(dead_code)]
    pub const fn program(self) -> &'static str {
        match self {
            Algorithm::Md5 => "md5sum",
            Algorithm::Sha1 => "sha1sum",
            Algorithm::Sha224 => "sha224sum",
            Algorithm::Sha256 => "sha256sum",
            Algorithm::Sha384 => "sha384sum",
            Algorithm::Sha512 => "sha512sum",
            Algorithm::Blake2b => "b2sum",
        }
    }
}

/// Compute `algorithm` over `data`, returning `(digest, length)`.
///
/// The array is 64 bytes because that is the largest digest here; only the
/// first `length` bytes are meaningful.
pub fn hash(algorithm: Algorithm, data: &[u8]) -> ([u8; 64], usize) {
    let mut out = [0u8; 64];
    match algorithm {
        Algorithm::Md5 => {
            let digest = md5(data);
            out[..16].copy_from_slice(&digest);
            (out, 16)
        }
        Algorithm::Sha1 => {
            let digest = sha1(data);
            out[..20].copy_from_slice(&digest);
            (out, 20)
        }
        Algorithm::Sha224 => {
            let digest = sha224(data);
            out[..28].copy_from_slice(&digest);
            (out, 28)
        }
        Algorithm::Sha256 => {
            let digest = sha256(data);
            out[..32].copy_from_slice(&digest);
            (out, 32)
        }
        Algorithm::Sha384 => {
            let digest = sha384(data);
            out[..48].copy_from_slice(&digest);
            (out, 48)
        }
        Algorithm::Sha512 => {
            let digest = sha512(data);
            out[..64].copy_from_slice(&digest);
            (out, 64)
        }
        Algorithm::Blake2b => {
            let digest = blake2b512(data);
            out[..64].copy_from_slice(&digest);
            (out, 64)
        }
    }
}

// ─── MD5 ──────────────────────────────────────────────────────────────────

const MD5_S: [u32; 64] = [
    7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9,
    14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10, 15,
    21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
];

const MD5_K: [u32; 64] = [
    0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613, 0xfd469501,
    0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be, 0x6b901122, 0xfd987193, 0xa679438e, 0x49b40821,
    0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa, 0xd62f105d, 0x02441453, 0xd8a1e681, 0xe7d3fbc8,
    0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed, 0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a,
    0xfffa3942, 0x8771f681, 0x6d9d6122, 0xfde5380c, 0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70,
    0x289b7ec6, 0xeaa127fa, 0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665,
    0xf4292244, 0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,
    0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1, 0xf7537e82, 0xbd3af235, 0x2ad7d2bb, 0xeb86d391,
];

fn md5(data: &[u8]) -> [u8; 16] {
    let mut state: [u32; 4] = [0x6745_2301, 0xefcd_ab89, 0x98ba_dcfe, 0x1032_5476];

    let full = data.len() / 64 * 64;
    let mut offset = 0usize;
    while offset < full {
        let mut block = [0u8; 64];
        block.copy_from_slice(&data[offset..offset + 64]);
        md5_block(&mut state, &block);
        offset += 64;
    }

    // 0x80, zero fill to 56 mod 64, then the 64-bit little-endian bit count.
    let remainder = data.len() - full;
    let mut tail = [0u8; 128];
    tail[..remainder].copy_from_slice(&data[full..]);
    tail[remainder] = 0x80;
    let tail_len = if remainder < 56 { 64 } else { 128 };
    let bits = (data.len() as u64).wrapping_mul(8);
    tail[tail_len - 8..tail_len].copy_from_slice(&bits.to_le_bytes());
    let mut position = 0usize;
    while position < tail_len {
        let mut block = [0u8; 64];
        block.copy_from_slice(&tail[position..position + 64]);
        md5_block(&mut state, &block);
        position += 64;
    }

    let mut out = [0u8; 16];
    for (index, word) in state.iter().enumerate() {
        out[index * 4..index * 4 + 4].copy_from_slice(&word.to_le_bytes());
    }
    out
}

fn md5_block(state: &mut [u32; 4], block: &[u8; 64]) {
    let mut words = [0u32; 16];
    for index in 0..16 {
        words[index] = u32::from_le_bytes([
            block[index * 4],
            block[index * 4 + 1],
            block[index * 4 + 2],
            block[index * 4 + 3],
        ]);
    }

    let [mut a, mut b, mut c, mut d] = *state;
    for round in 0..64 {
        let (shift, mix) = match round / 16 {
            0 => (round, (b & c) | (!b & d)),
            1 => ((5 * round + 1) % 16, (d & b) | (!d & c)),
            2 => ((3 * round + 5) % 16, b ^ c ^ d),
            _ => ((7 * round) % 16, c ^ (b | !d)),
        };
        let sum = a
            .wrapping_add(mix)
            .wrapping_add(MD5_K[round])
            .wrapping_add(words[shift]);
        a = d;
        d = c;
        c = b;
        b = b.wrapping_add(sum.rotate_left(MD5_S[round]));
    }
    state[0] = state[0].wrapping_add(a);
    state[1] = state[1].wrapping_add(b);
    state[2] = state[2].wrapping_add(c);
    state[3] = state[3].wrapping_add(d);
}

// ─── SHA-1 ────────────────────────────────────────────────────────────────

fn sha1(data: &[u8]) -> [u8; 20] {
    let mut state: [u32; 5] = [0x6745_2301, 0xefcd_ab89, 0x98ba_dcfe, 0x1032_5476, 0xc3d2_e1f0];

    let full = data.len() / 64 * 64;
    let mut offset = 0usize;
    while offset < full {
        let mut block = [0u8; 64];
        block.copy_from_slice(&data[offset..offset + 64]);
        sha1_block(&mut state, &block);
        offset += 64;
    }

    let remainder = data.len() - full;
    let mut tail = [0u8; 128];
    tail[..remainder].copy_from_slice(&data[full..]);
    tail[remainder] = 0x80;
    let tail_len = if remainder < 56 { 64 } else { 128 };
    let bits = (data.len() as u64).wrapping_mul(8);
    tail[tail_len - 8..tail_len].copy_from_slice(&bits.to_be_bytes());
    let mut position = 0usize;
    while position < tail_len {
        let mut block = [0u8; 64];
        block.copy_from_slice(&tail[position..position + 64]);
        sha1_block(&mut state, &block);
        position += 64;
    }

    let mut out = [0u8; 20];
    for (index, word) in state.iter().enumerate() {
        out[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

fn sha1_block(state: &mut [u32; 5], block: &[u8; 64]) {
    let mut words = [0u32; 80];
    for index in 0..16 {
        words[index] = u32::from_be_bytes([
            block[index * 4],
            block[index * 4 + 1],
            block[index * 4 + 2],
            block[index * 4 + 3],
        ]);
    }
    for index in 16..80 {
        words[index] = (words[index - 3] ^ words[index - 8] ^ words[index - 14] ^ words[index - 16])
            .rotate_left(1);
    }

    let [mut a, mut b, mut c, mut d, mut e] = *state;
    for (round, word) in words.iter().enumerate() {
        let (f, k) = match round {
            0..=19 => ((b & c) | (!b & d), 0x5a82_7999u32),
            20..=39 => (b ^ c ^ d, 0x6ed9_eba1),
            40..=59 => ((b & c) | (b & d) | (c & d), 0x8f1b_bcdc),
            _ => (b ^ c ^ d, 0xca62_c1d6),
        };
        let temporary = a
            .rotate_left(5)
            .wrapping_add(f)
            .wrapping_add(e)
            .wrapping_add(k)
            .wrapping_add(*word);
        e = d;
        d = c;
        c = b.rotate_left(30);
        b = a;
        a = temporary;
    }
    state[0] = state[0].wrapping_add(a);
    state[1] = state[1].wrapping_add(b);
    state[2] = state[2].wrapping_add(c);
    state[3] = state[3].wrapping_add(d);
    state[4] = state[4].wrapping_add(e);
}

// ─── SHA-256 ──────────────────────────────────────────────────────────────

const SHA256_K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

const SHA256_IV: [u32; 8] = [
    0x6a09_e667, 0xbb67_ae85, 0x3c6e_f372, 0xa54f_f53a, 0x510e_527f, 0x9b05_688c, 0x1f83_d9ab,
    0x5be0_cd19,
];

/// FIPS 180-4 SHA-224 initial state. SHA-224 shares the SHA-256 compression
/// function and differs only in the IV and in truncating the output to 28
/// bytes (the last 32-bit word of the SHA-256 state is discarded).
const SHA224_IV: [u32; 8] = [
    0xc105_9ed8, 0x367c_d507, 0x3070_dd17, 0xf70e_5939, 0xffc0_0b31, 0x6858_1511, 0x64f9_8fa7,
    0xbefa_4fa4,
];

fn sha256(data: &[u8]) -> [u8; 32] {
    sha256_with_iv(data, SHA256_IV)
}

fn sha224(data: &[u8]) -> [u8; 28] {
    let digest = sha256_with_iv(data, SHA224_IV);
    let mut out = [0u8; 28];
    out.copy_from_slice(&digest[..28]);
    out
}

fn sha256_with_iv(data: &[u8], mut state: [u32; 8]) -> [u8; 32] {
    let full = data.len() / 64 * 64;
    let mut offset = 0usize;
    while offset < full {
        let mut block = [0u8; 64];
        block.copy_from_slice(&data[offset..offset + 64]);
        sha256_block(&mut state, &block);
        offset += 64;
    }

    let remainder = data.len() - full;
    let mut tail = [0u8; 128];
    tail[..remainder].copy_from_slice(&data[full..]);
    tail[remainder] = 0x80;
    let tail_len = if remainder < 56 { 64 } else { 128 };
    let bits = (data.len() as u64).wrapping_mul(8);
    tail[tail_len - 8..tail_len].copy_from_slice(&bits.to_be_bytes());
    let mut position = 0usize;
    while position < tail_len {
        let mut block = [0u8; 64];
        block.copy_from_slice(&tail[position..position + 64]);
        sha256_block(&mut state, &block);
        position += 64;
    }

    let mut out = [0u8; 32];
    for (index, word) in state.iter().enumerate() {
        out[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

fn sha256_block(state: &mut [u32; 8], block: &[u8; 64]) {
    let mut words = [0u32; 64];
    for index in 0..16 {
        words[index] = u32::from_be_bytes([
            block[index * 4],
            block[index * 4 + 1],
            block[index * 4 + 2],
            block[index * 4 + 3],
        ]);
    }
    for index in 16..64 {
        let s0 = words[index - 15].rotate_right(7)
            ^ words[index - 15].rotate_right(18)
            ^ (words[index - 15] >> 3);
        let s1 = words[index - 2].rotate_right(17)
            ^ words[index - 2].rotate_right(19)
            ^ (words[index - 2] >> 10);
        words[index] = words[index - 16]
            .wrapping_add(s0)
            .wrapping_add(words[index - 7])
            .wrapping_add(s1);
    }

    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = *state;
    for round in 0..64 {
        let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let choose = (e & f) ^ (!e & g);
        let temp1 = h
            .wrapping_add(s1)
            .wrapping_add(choose)
            .wrapping_add(SHA256_K[round])
            .wrapping_add(words[round]);
        let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let majority = (a & b) ^ (a & c) ^ (b & c);
        let temp2 = s0.wrapping_add(majority);

        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(temp1);
        d = c;
        c = b;
        b = a;
        a = temp1.wrapping_add(temp2);
    }
    state[0] = state[0].wrapping_add(a);
    state[1] = state[1].wrapping_add(b);
    state[2] = state[2].wrapping_add(c);
    state[3] = state[3].wrapping_add(d);
    state[4] = state[4].wrapping_add(e);
    state[5] = state[5].wrapping_add(f);
    state[6] = state[6].wrapping_add(g);
    state[7] = state[7].wrapping_add(h);
}

// ─── SHA-512 ──────────────────────────────────────────────────────────────

const SHA512_K: [u64; 80] = [
    0x428a2f98d728ae22, 0x7137449123ef65cd, 0xb5c0fbcfec4d3b2f, 0xe9b5dba58189dbbc,
    0x3956c25bf348b538, 0x59f111f1b605d019, 0x923f82a4af194f9b, 0xab1c5ed5da6d8118,
    0xd807aa98a3030242, 0x12835b0145706fbe, 0x243185be4ee4b28c, 0x550c7dc3d5ffb4e2,
    0x72be5d74f27b896f, 0x80deb1fe3b1696b1, 0x9bdc06a725c71235, 0xc19bf174cf692694,
    0xe49b69c19ef14ad2, 0xefbe4786384f25e3, 0x0fc19dc68b8cd5b5, 0x240ca1cc77ac9c65,
    0x2de92c6f592b0275, 0x4a7484aa6ea6e483, 0x5cb0a9dcbd41fbd4, 0x76f988da831153b5,
    0x983e5152ee66dfab, 0xa831c66d2db43210, 0xb00327c898fb213f, 0xbf597fc7beef0ee4,
    0xc6e00bf33da88fc2, 0xd5a79147930aa725, 0x06ca6351e003826f, 0x142929670a0e6e70,
    0x27b70a8546d22ffc, 0x2e1b21385c26c926, 0x4d2c6dfc5ac42aed, 0x53380d139d95b3df,
    0x650a73548baf63de, 0x766a0abb3c77b2a8, 0x81c2c92e47edaee6, 0x92722c851482353b,
    0xa2bfe8a14cf10364, 0xa81a664bbc423001, 0xc24b8b70d0f89791, 0xc76c51a30654be30,
    0xd192e819d6ef5218, 0xd69906245565a910, 0xf40e35855771202a, 0x106aa07032bbd1b8,
    0x19a4c116b8d2d0c8, 0x1e376c085141ab53, 0x2748774cdf8eeb99, 0x34b0bcb5e19b48a8,
    0x391c0cb3c5c95a63, 0x4ed8aa4ae3418acb, 0x5b9cca4f7763e373, 0x682e6ff3d6b2b8a3,
    0x748f82ee5defb2fc, 0x78a5636f43172f60, 0x84c87814a1f0ab72, 0x8cc702081a6439ec,
    0x90befffa23631e28, 0xa4506cebde82bde9, 0xbef9a3f7b2c67915, 0xc67178f2e372532b,
    0xca273eceea26619c, 0xd186b8c721c0c207, 0xeada7dd6cde0eb1e, 0xf57d4f7fee6ed178,
    0x06f067aa72176fba, 0x0a637dc5a2c898a6, 0x113f9804bef90dae, 0x1b710b35131c471b,
    0x28db77f523047d84, 0x32caab7b40c72493, 0x3c9ebe0a15c9bebc, 0x431d67c49c100d4c,
    0x4cc5d4becb3e42b6, 0x597f299cfc657e2a, 0x5fcb6fab3ad6faec, 0x6c44198c4a475817,
];

const SHA512_IV: [u64; 8] = [
    0x6a09e667f3bcc908,
    0xbb67ae8584caa73b,
    0x3c6ef372fe94f82b,
    0xa54ff53a5f1d36f1,
    0x510e527fade682d1,
    0x9b05688c2b3e6c1f,
    0x1f83d9abfb41bd6b,
    0x5be0cd19137e2179,
];

/// FIPS 180-4 SHA-384 initial state. SHA-384 shares the SHA-512 compression
/// function and differs only in the IV and in truncating the output to 48
/// bytes (the last two 64-bit words of the SHA-512 state are discarded).
const SHA384_IV: [u64; 8] = [
    0xcbbb9d5dc1059ed8,
    0x629a292a367cd507,
    0x9159015a3070dd17,
    0x152fecd8f70e5939,
    0x67332667ffc00b31,
    0x8eb44a8768581511,
    0xdb0c2e0d64f98fa7,
    0x47b5481dbefa4fa4,
];

fn sha512(data: &[u8]) -> [u8; 64] {
    sha512_with_iv(data, SHA512_IV)
}

fn sha384(data: &[u8]) -> [u8; 48] {
    let digest = sha512_with_iv(data, SHA384_IV);
    let mut out = [0u8; 48];
    out.copy_from_slice(&digest[..48]);
    out
}

fn sha512_with_iv(data: &[u8], mut state: [u64; 8]) -> [u8; 64] {
    let full = data.len() / 128 * 128;
    let mut offset = 0usize;
    while offset < full {
        let mut block = [0u8; 128];
        block.copy_from_slice(&data[offset..offset + 128]);
        sha512_block(&mut state, &block);
        offset += 128;
    }

    let remainder = data.len() - full;
    let mut tail = [0u8; 256];
    tail[..remainder].copy_from_slice(&data[full..]);
    tail[remainder] = 0x80;
    let tail_len = if remainder < 112 { 128 } else { 256 };
    let bits = (data.len() as u128).wrapping_mul(8);
    tail[tail_len - 16..tail_len].copy_from_slice(&bits.to_be_bytes());
    let mut position = 0usize;
    while position < tail_len {
        let mut block = [0u8; 128];
        block.copy_from_slice(&tail[position..position + 128]);
        sha512_block(&mut state, &block);
        position += 128;
    }

    let mut out = [0u8; 64];
    for (index, word) in state.iter().enumerate() {
        out[index * 8..index * 8 + 8].copy_from_slice(&word.to_be_bytes());
    }
    out
}

fn sha512_block(state: &mut [u64; 8], block: &[u8; 128]) {
    let mut words = [0u64; 80];
    for index in 0..16 {
        words[index] = u64::from_be_bytes([
            block[index * 8],
            block[index * 8 + 1],
            block[index * 8 + 2],
            block[index * 8 + 3],
            block[index * 8 + 4],
            block[index * 8 + 5],
            block[index * 8 + 6],
            block[index * 8 + 7],
        ]);
    }
    for index in 16..80 {
        let s0 = words[index - 15].rotate_right(1)
            ^ words[index - 15].rotate_right(8)
            ^ (words[index - 15] >> 7);
        let s1 = words[index - 2].rotate_right(19)
            ^ words[index - 2].rotate_right(61)
            ^ (words[index - 2] >> 6);
        words[index] = words[index - 16]
            .wrapping_add(s0)
            .wrapping_add(words[index - 7])
            .wrapping_add(s1);
    }

    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = *state;
    for round in 0..80 {
        let s1 = e.rotate_right(14) ^ e.rotate_right(18) ^ e.rotate_right(41);
        let choose = (e & f) ^ (!e & g);
        let temp1 = h
            .wrapping_add(s1)
            .wrapping_add(choose)
            .wrapping_add(SHA512_K[round])
            .wrapping_add(words[round]);
        let s0 = a.rotate_right(28) ^ a.rotate_right(34) ^ a.rotate_right(39);
        let majority = (a & b) ^ (a & c) ^ (b & c);
        let temp2 = s0.wrapping_add(majority);

        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(temp1);
        d = c;
        c = b;
        b = a;
        a = temp1.wrapping_add(temp2);
    }
    state[0] = state[0].wrapping_add(a);
    state[1] = state[1].wrapping_add(b);
    state[2] = state[2].wrapping_add(c);
    state[3] = state[3].wrapping_add(d);
    state[4] = state[4].wrapping_add(e);
    state[5] = state[5].wrapping_add(f);
    state[6] = state[6].wrapping_add(g);
    state[7] = state[7].wrapping_add(h);
}

// ─── BLAKE2b-512 ──────────────────────────────────────────────────────────

const BLAKE2B_IV: [u64; 8] = [
    0x6a09e667f3bcc908,
    0xbb67ae8584caa73b,
    0x3c6ef372fe94f82b,
    0xa54ff53a5f1d36f1,
    0x510e527fade682d1,
    0x9b05688c2b3e6c1f,
    0x1f83d9abfb41bd6b,
    0x5be0cd19137e2179,
];

const BLAKE2B_SIGMA: [[usize; 16]; 10] = [
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
    [14, 10, 4, 8, 9, 15, 13, 6, 1, 12, 0, 2, 11, 7, 5, 3],
    [11, 8, 12, 0, 5, 2, 15, 13, 10, 14, 3, 6, 7, 1, 9, 4],
    [7, 9, 3, 1, 13, 12, 11, 14, 2, 6, 5, 10, 4, 0, 15, 8],
    [9, 0, 5, 7, 2, 4, 10, 15, 14, 1, 11, 12, 6, 8, 3, 13],
    [2, 12, 6, 10, 0, 11, 8, 3, 4, 13, 7, 5, 15, 14, 1, 9],
    [12, 5, 1, 15, 14, 13, 4, 10, 0, 7, 6, 3, 9, 2, 8, 11],
    [13, 11, 7, 14, 12, 1, 3, 9, 5, 0, 15, 4, 8, 6, 2, 10],
    [6, 15, 14, 9, 11, 3, 0, 8, 12, 2, 13, 7, 1, 4, 10, 5],
    [10, 2, 8, 4, 7, 6, 1, 5, 15, 11, 9, 14, 3, 12, 13, 0],
];

fn blake2b512(data: &[u8]) -> [u8; 64] {
    // Unkeyed, 64-byte digest: h[0] ^= 0x01010000 ^ (key_len << 8) ^ out_len.
    let mut state = BLAKE2B_IV;
    state[0] ^= 0x0101_0000 ^ 64;

    // Leave the last (possibly partial) block for the final compression.
    let mut offset = 0usize;
    let mut counter: u128 = 0;
    while data.len().saturating_sub(offset) > 128 {
        let mut block = [0u8; 128];
        block.copy_from_slice(&data[offset..offset + 128]);
        counter = counter.wrapping_add(128);
        blake2b_compress(&mut state, &block, counter, false);
        offset += 128;
    }

    let remainder = data.len().saturating_sub(offset);
    let mut block = [0u8; 128];
    block[..remainder].copy_from_slice(&data[offset..]);
    counter = counter.wrapping_add(remainder as u128);
    blake2b_compress(&mut state, &block, counter, true);

    let mut out = [0u8; 64];
    for (index, word) in state.iter().enumerate() {
        out[index * 8..index * 8 + 8].copy_from_slice(&word.to_le_bytes());
    }
    out
}

fn blake2b_compress(state: &mut [u64; 8], block: &[u8; 128], counter: u128, last: bool) {
    let mut message = [0u64; 16];
    for index in 0..16 {
        message[index] = u64::from_le_bytes([
            block[index * 8],
            block[index * 8 + 1],
            block[index * 8 + 2],
            block[index * 8 + 3],
            block[index * 8 + 4],
            block[index * 8 + 5],
            block[index * 8 + 6],
            block[index * 8 + 7],
        ]);
    }

    let mut work = [0u64; 16];
    work[..8].copy_from_slice(state);
    work[8..].copy_from_slice(&BLAKE2B_IV);
    work[12] ^= counter as u64;
    work[13] ^= (counter >> 64) as u64;
    if last {
        work[14] = !work[14];
    }

    for round in 0..12 {
        let sigma = BLAKE2B_SIGMA[round % 10];
        blake2b_g(&mut work, 0, 4, 8, 12, message[sigma[0]], message[sigma[1]]);
        blake2b_g(&mut work, 1, 5, 9, 13, message[sigma[2]], message[sigma[3]]);
        blake2b_g(&mut work, 2, 6, 10, 14, message[sigma[4]], message[sigma[5]]);
        blake2b_g(&mut work, 3, 7, 11, 15, message[sigma[6]], message[sigma[7]]);
        blake2b_g(&mut work, 0, 5, 10, 15, message[sigma[8]], message[sigma[9]]);
        blake2b_g(&mut work, 1, 6, 11, 12, message[sigma[10]], message[sigma[11]]);
        blake2b_g(&mut work, 2, 7, 8, 13, message[sigma[12]], message[sigma[13]]);
        blake2b_g(&mut work, 3, 4, 9, 14, message[sigma[14]], message[sigma[15]]);
    }

    for index in 0..8 {
        state[index] ^= work[index] ^ work[index + 8];
    }
}

fn blake2b_g(
    work: &mut [u64; 16],
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    first: u64,
    second: u64,
) {
    work[a] = work[a].wrapping_add(work[b]).wrapping_add(first);
    work[d] = (work[d] ^ work[a]).rotate_right(32);
    work[c] = work[c].wrapping_add(work[d]);
    work[b] = (work[b] ^ work[c]).rotate_right(24);
    work[a] = work[a].wrapping_add(work[b]).wrapping_add(second);
    work[d] = (work[d] ^ work[a]).rotate_right(16);
    work[c] = work[c].wrapping_add(work[d]);
    work[b] = (work[b] ^ work[c]).rotate_right(63);
}