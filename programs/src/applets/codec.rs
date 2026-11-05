//! Generic RFC 4648 base-N codec used by `base64` and `base32`.
//!
//! Only the two widths the bundle needs are used (6 bits / 4-character block
//! and 5 bits / 8-character block), but the implementation is parameterized so
//! the padding and leftover-bit rules live in one place instead of two
//! slightly different copies.

use alloc::vec::Vec;

pub const BASE64: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
pub const BASE32: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

pub enum CodecError {
    Invalid,
    Memory,
}

/// Encode `data` with `alphabet` and `bits` per character into a padded block
/// multiple. `None` is an allocation failure.
pub fn encode(data: &[u8], alphabet: &[u8], bits: u32, block: usize) -> Option<Vec<u8>> {
    let characters = (data.len().checked_mul(8)? + bits as usize - 1) / bits as usize;
    let padded = characters.div_ceil(block).checked_mul(block)?;
    let mut out: Vec<u8> = Vec::new();
    out.try_reserve(padded).ok()?;

    let mut accumulator: u32 = 0;
    let mut available: u32 = 0;
    for byte in data {
        accumulator = (accumulator << 8) | (*byte as u32);
        available += 8;
        while available >= bits {
            available -= bits;
            let index = ((accumulator >> available) & ((1u32 << bits) - 1)) as usize;
            out.push(alphabet[index]);
        }
    }
    if available > 0 {
        let index = ((accumulator << (bits - available)) & ((1u32 << bits) - 1)) as usize;
        out.push(alphabet[index]);
    }
    while out.len() < padded {
        out.push(b'=');
    }
    Some(out)
}

/// Decode `data`, ignoring ASCII whitespace and requiring canonical padding.
pub fn decode(data: &[u8], alphabet: &[u8], bits: u32, block: usize) -> Result<Vec<u8>, CodecError> {
    let mut input: Vec<u8> = Vec::new();
    input
        .try_reserve(data.len())
        .map_err(|_| CodecError::Memory)?;
    for byte in data {
        if byte.is_ascii_whitespace() {
            continue;
        }
        input.push(*byte);
    }
    if input.is_empty() {
        return Ok(Vec::new());
    }
    if input.len() % block != 0 {
        return Err(CodecError::Invalid);
    }

    let mut out: Vec<u8> = Vec::new();
    out.try_reserve(input.len().saturating_mul(bits as usize) / 8)
        .map_err(|_| CodecError::Memory)?;

    let blocks = input.len() / block;
    for (number, chunk) in input.chunks(block).enumerate() {
        let last = number + 1 == blocks;
        let mut padding = 0usize;
        let mut data_characters = 0usize;
        let mut seen_padding = false;
        for byte in chunk {
            if *byte == b'=' {
                seen_padding = true;
                padding += 1;
            } else {
                if seen_padding {
                    return Err(CodecError::Invalid);
                }
                if !alphabet.contains(byte) {
                    return Err(CodecError::Invalid);
                }
                data_characters += 1;
            }
        }
        if !last && padding != 0 {
            return Err(CodecError::Invalid);
        }
        if data_characters == 0 || data_characters + padding != block {
            return Err(CodecError::Invalid);
        }

        let mut accumulator: u32 = 0;
        let mut available: u32 = 0;
        for byte in &chunk[..data_characters] {
            let value = alphabet
                .iter()
                .position(|candidate| candidate == byte)
                .ok_or(CodecError::Invalid)? as u32;
            accumulator = (accumulator << bits) | value;
            available += bits;
            while available >= 8 {
                available -= 8;
                out.push(((accumulator >> available) & 0xff) as u8);
            }
        }

        if last {
            if available >= bits {
                return Err(CodecError::Invalid);
            }
            if available > 0 && (accumulator & ((1u32 << available) - 1)) != 0 {
                return Err(CodecError::Invalid);
            }
        }
    }

    Ok(out)
}