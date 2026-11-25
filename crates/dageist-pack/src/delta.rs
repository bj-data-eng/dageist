//! Git's delta format: how a target is made from a base.
//!
//! A delta starts with the base's length and the target's, each as a little-endian number of
//! seven bits a byte, the eighth bit saying another byte follows. Then come instructions, each
//! starting with one byte:
//!
//! - **Copy** (high bit set): copy a range of the base. The low four bits say which bytes of the
//!   offset follow, the next three which bytes of the size, least significant first; a byte
//!   left out is zero, and a size of zero means `0x10000`.
//! - **Insert** (high bit clear, not zero): the byte is a count, 1 to 127, of new bytes that
//!   follow it and are written as they are.
//!
//! The encoder finds copies the way git's does: it indexes the base by blocks of [`BLOCK`]
//! bytes, looks up every [`BLOCK`]-byte window of the target, and grows each match it finds
//! forwards and back as far as the bytes agree.

use std::collections::HashMap;
use std::fmt;

/// How many bytes a match starts from. A shorter match costs more as a copy than as an insert.
pub const BLOCK: usize = 16;
/// The most bytes one copy instruction copies.
const MAX_COPY: usize = 0x1_0000;
/// The most bytes one insert instruction holds.
const MAX_INSERT: usize = 0x7f;
/// The most places of the base one block's contents are remembered at, so that a base that
/// repeats itself does not make every lookup slow.
const MAX_CANDIDATES: usize = 64;

/// The base is too large for a copy instruction to reach every byte of it: a copy's offset has
/// four bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BaseTooLarge {
    /// The base's length.
    pub len: usize,
}

impl fmt::Display for BaseTooLarge {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "a base of {} bytes is past the 4 GiB a delta can copy from",
            self.len
        )
    }
}

impl std::error::Error for BaseTooLarge {}

/// Writes `value` as a delta header's size: seven bits a byte, least significant first.
fn put_size(out: &mut Vec<u8>, mut value: usize) {
    loop {
        // Masked to seven bits, so the cast keeps every bit.
        let low = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            out.push(low);
            return;
        }
        out.push(low | 0x80);
    }
}

/// Writes instructions that insert `bytes`.
fn put_insert(out: &mut Vec<u8>, bytes: &[u8]) {
    for chunk in bytes.chunks(MAX_INSERT) {
        // A chunk holds at most 127 bytes.
        out.push(chunk.len() as u8);
        out.extend_from_slice(chunk);
    }
}

/// Writes instructions that copy `len` bytes of the base from `offset`.
fn put_copy(out: &mut Vec<u8>, mut offset: usize, mut len: usize) {
    while len > 0 {
        let size = len.min(MAX_COPY);
        let mut op = 0x80_u8;
        let mut fields = [0_u8; 7];
        let mut written = 0;
        for (place, bit) in (0..4).zip([0x01, 0x02, 0x04, 0x08]) {
            // One byte of the offset, which is below 4 GiB.
            let byte = (offset >> (8 * place)) as u8;
            if byte != 0 {
                op |= bit;
                fields[written] = byte;
                written += 1;
            }
        }
        // A size of 0x10000 is written as no size bytes at all.
        let written_size = if size == MAX_COPY { 0 } else { size };
        for (place, bit) in (0..3).zip([0x10, 0x20, 0x40]) {
            let byte = (written_size >> (8 * place)) as u8;
            if byte != 0 {
                op |= bit;
                fields[written] = byte;
                written += 1;
            }
        }
        out.push(op);
        out.extend_from_slice(&fields[..written]);
        offset += size;
        len -= size;
    }
}

/// A hash of one block, which a match is looked up by. Equal blocks have equal hashes; the bytes
/// are compared before a match is taken.
fn block_hash(block: &[u8]) -> u64 {
    // FNV-1a.
    block.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// Where each block of the base starts, by its hash.
struct BaseIndex {
    starts: HashMap<u64, Vec<usize>>,
}

impl BaseIndex {
    fn of(base: &[u8]) -> Self {
        let mut starts: HashMap<u64, Vec<usize>> = HashMap::new();
        for start in (0..base.len().saturating_sub(BLOCK - 1)).step_by(BLOCK) {
            let places = starts
                .entry(block_hash(&base[start..start + BLOCK]))
                .or_default();
            if places.len() < MAX_CANDIDATES {
                places.push(start);
            }
        }
        Self { starts }
    }

    /// The longest match of the target from `at` in the base, as where it starts in the base and
    /// how long it is, when one starts with a whole block.
    fn longest(&self, base: &[u8], target: &[u8], at: usize) -> Option<(usize, usize)> {
        let window = &target[at..at + BLOCK];
        let places = self.starts.get(&block_hash(window))?;
        let mut best: Option<(usize, usize)> = None;
        for &from in places {
            if &base[from..from + BLOCK] != window {
                continue;
            }
            let len = BLOCK
                + base[from + BLOCK..]
                    .iter()
                    .zip(&target[at + BLOCK..])
                    .take_while(|(left, right)| left == right)
                    .count();
            if best.is_none_or(|(_, longest)| len > longest) {
                best = Some((from, len));
            }
        }
        best
    }
}

/// Returns the delta that makes `target` from `base`.
///
/// # Errors
///
/// When `base` is 4 GiB or longer, which a copy's offset cannot reach.
pub fn encode(base: &[u8], target: &[u8]) -> Result<Vec<u8>, BaseTooLarge> {
    if u32::try_from(base.len()).is_err() {
        return Err(BaseTooLarge { len: base.len() });
    }
    let mut out = Vec::with_capacity(target.len() / 8 + 16);
    put_size(&mut out, base.len());
    put_size(&mut out, target.len());
    let index = BaseIndex::of(base);
    // The target's bytes from `pending` up to `at` are not written yet, and are inserted unless a
    // match grows back over them.
    let mut pending = 0;
    let mut at = 0;
    while at + BLOCK <= target.len() {
        let Some((mut from, len)) = index.longest(base, target, at) else {
            at += 1;
            continue;
        };
        let mut start = at;
        while start > pending && from > 0 && base[from - 1] == target[start - 1] {
            start -= 1;
            from -= 1;
        }
        let len = len + (at - start);
        put_insert(&mut out, &target[pending..start]);
        put_copy(&mut out, from, len);
        at = start + len;
        pending = at;
    }
    put_insert(&mut out, &target[pending..]);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::{BLOCK, encode, put_copy, put_size};

    /// Makes the target a delta describes from its base, by git's rules as the module states
    /// them: written apart from the encoder, so that the encoder is not checked against itself.
    fn apply(base: &[u8], delta: &[u8]) -> Vec<u8> {
        let mut at = 0;
        let mut size = || {
            let mut value = 0_usize;
            let mut shift = 0;
            loop {
                let byte = delta[at];
                at += 1;
                value |= usize::from(byte & 0x7f) << shift;
                shift += 7;
                if byte & 0x80 == 0 {
                    return value;
                }
            }
        };
        assert_eq!(size(), base.len(), "the delta names its base's length");
        let target_len = size();
        let mut out = Vec::with_capacity(target_len);
        while at < delta.len() {
            let op = delta[at];
            at += 1;
            if op & 0x80 != 0 {
                let mut field = |bit: u8, shift: usize| {
                    if op & bit == 0 {
                        return 0;
                    }
                    let byte = usize::from(delta[at]);
                    at += 1;
                    byte << shift
                };
                let offset = field(0x01, 0) | field(0x02, 8) | field(0x04, 16) | field(0x08, 24);
                let mut len = field(0x10, 0) | field(0x20, 8) | field(0x40, 16);
                if len == 0 {
                    len = 0x1_0000;
                }
                out.extend_from_slice(&base[offset..offset + len]);
            } else {
                assert_ne!(op, 0, "no instruction is zero");
                let len = usize::from(op);
                out.extend_from_slice(&delta[at..at + len]);
                at += len;
            }
        }
        assert_eq!(out.len(), target_len, "the delta names its target's length");
        out
    }

    fn round_trip(base: &[u8], target: &[u8]) -> Vec<u8> {
        let delta = encode(base, target).unwrap();
        assert_eq!(apply(base, &delta), target);
        delta
    }

    #[test]
    fn sizes_are_written_seven_bits_a_byte() {
        let mut out = Vec::new();
        put_size(&mut out, 0);
        put_size(&mut out, 127);
        put_size(&mut out, 128);
        put_size(&mut out, 300);
        assert_eq!(out, [0x00, 0x7f, 0x80, 0x01, 0xac, 0x02]);
    }

    #[test]
    fn a_copy_writes_only_the_bytes_that_are_not_zero_and_splits_at_0x10000() {
        let mut out = Vec::new();
        put_copy(&mut out, 0x0001_0200, 0x10);
        assert_eq!(out, [0x80 | 0x02 | 0x04 | 0x10, 0x02, 0x01, 0x10]);
        let mut out = Vec::new();
        put_copy(&mut out, 0, 0x1_0001);
        // 0x10000 from offset 0 with no fields, then 1 byte from 0x10000.
        assert_eq!(out, [0x80, 0x80 | 0x04 | 0x10, 0x01, 0x01]);
    }

    #[test]
    fn a_target_equal_to_its_base_is_one_copy() {
        let base = b"0123456789abcdefghijklmnopqrstuvwxyz".repeat(10);
        let delta = round_trip(&base, &base);
        assert!(delta.len() < 12, "{} bytes", delta.len());
    }

    #[test]
    fn edge_cases_round_trip() {
        round_trip(b"", b"");
        round_trip(b"", b"new bytes only");
        round_trip(b"a base nothing is taken from", b"");
        round_trip(b"short", b"short");
        round_trip(&[7; 1000], &[7; 1000]);
        round_trip(&[7; 1000], &[8; 300]);
    }

    #[test]
    fn a_small_edit_in_a_large_document_is_a_small_delta() {
        let base: Vec<u8> = (0..20_000_u32)
            .flat_map(|n| format!("{{\"step\": {n}, \"value\": \"{}\"}},\n", n * 7).into_bytes())
            .collect();
        let mut target = base.clone();
        let middle = target.len() / 2;
        target.splice(middle..middle + 10, b"an edited value".iter().copied());
        let delta = round_trip(&base, &target);
        assert!(delta.len() < 64, "{} bytes for one edit", delta.len());
    }

    #[test]
    fn insertions_deletions_and_moves_round_trip() {
        let base = b"the quick brown fox jumps over the lazy dog; ".repeat(40);
        let mut moved = base[base.len() / 2..].to_vec();
        moved.extend_from_slice(&base[..base.len() / 2]);
        round_trip(&base, &moved);
        let mut cut = base.clone();
        cut.drain(100..400);
        round_trip(&base, &cut);
        let mut grown = base.clone();
        grown.splice(
            BLOCK * 3..BLOCK * 3,
            b"something new in the middle".iter().copied(),
        );
        round_trip(&base, &grown);
    }

    #[test]
    fn a_copy_longer_than_0x10000_round_trips() {
        let base: Vec<u8> = (0..200_000_u32).map(|n| (n * 31 % 251) as u8).collect();
        round_trip(&base, &base);
        round_trip(&base, &base[1000..190_000]);
    }
}
