//! A QR code for the addresses this app hands out (M-3).
//!
//! Two of this app's features produce a URL whose whole point is that it is
//! opened on **another device**: [`crate::lan`] gives a phone on the same Wi-Fi
//! a name for the project, and [`crate::tunnel`] gives the internet one. Both
//! are long, both contain a dashed IP address or a random Cloudflare word
//! salad, and today the only way to get either onto a phone is to type it —
//! which is exactly the moment somebody gives up and uses the desktop
//! browser's device emulation instead.
//!
//! ## Why this is written here rather than pulled in
//!
//! A QR encoder is a dependency in every language, and taking one would have
//! been the shorter route. Two things argued against it for this particular
//! job. The first is that the encoders are not small — the popular Rust one
//! pulls in an image stack for rendering nobody here wants, because this draws
//! into an SVG in the front end. The second is that a QR code is a **closed,
//! finished specification** with published test vectors: unlike a parser for
//! somebody else's format, it cannot acquire new cases later, so the usual
//! argument for a maintained dependency — that the world will change under you
//! — does not apply.
//!
//! What made it defensible was having something to check it against. The
//! encoder below is verified two ways: against ISO/IEC 18004's own worked
//! example, and — in `examples/qr_probe.rs` — by handing what it produced to
//! **macOS's own decoder** and asking what it reads. An encoder checked only
//! against its author's expectations is an encoder that agrees with its author.
//!
//! ## What is deliberately not here
//!
//! * **Numeric and alphanumeric modes.** They pack denser, and a URL is neither
//!   — `https://` alone is outside the alphanumeric character set. Byte mode is
//!   the only one that can carry what this app produces, so a second encoder
//!   for the case that never happens would be untested code by construction.
//! * **Error correction beyond level M.** M recovers 15% and is what almost
//!   every generator emits. A level is a trade against size, and offering four
//!   would be a setting nobody can answer.
//! * **Versions above 10.** Ten holds 213 bytes in byte mode, and the longest
//!   thing this app produces is a `trycloudflare.com` URL at around sixty.
//!   Stopping there keeps the block table — the part of this that is transcribed
//!   rather than derived — short enough to check by hand, and every number in
//!   it is checked against the version's total codeword count in a test.

use crate::error::{Code, Error, Result};
use serde::Serialize;

/// The largest version this draws, and the reason the tables end where they do.
const MAX_VERSION: usize = 10;

/// Total codewords per version — data plus error correction, level-independent.
///
/// Kept beside the block table so a mistyped block figure cannot go unnoticed:
/// blocks × (data + ecc) must land exactly on the row for that version, and a
/// test asserts it for all ten.
const TOTAL_CODEWORDS: [usize; MAX_VERSION] = [26, 44, 70, 100, 134, 172, 196, 242, 292, 346];

/// Error-correction level M: `(ecc per block, blocks, data, blocks, data)`.
///
/// Two groups because most versions split their data into blocks of two
/// different sizes; where they do not, the second group is empty.
const BLOCKS_M: [(usize, usize, usize, usize, usize); MAX_VERSION] = [
    (10, 1, 16, 0, 0),
    (16, 1, 28, 0, 0),
    (26, 1, 44, 0, 0),
    (18, 2, 32, 0, 0),
    (24, 2, 43, 0, 0),
    (16, 4, 27, 0, 0),
    (18, 4, 31, 0, 0),
    (22, 2, 38, 2, 39),
    (22, 3, 36, 2, 37),
    (26, 4, 43, 1, 44),
];

/// Alignment pattern centre coordinates per version. Version 1 has none.
const ALIGNMENT: [&[usize]; MAX_VERSION] = [
    &[],
    &[6, 18],
    &[6, 22],
    &[6, 26],
    &[6, 30],
    &[6, 34],
    &[6, 22, 38],
    &[6, 24, 42],
    &[6, 26, 46],
    &[6, 28, 50],
];

/// Bits left over after the codewords, which are written as zeros.
///
/// Not a rounding artefact: the symbol has a fixed module count and the
/// codewords do not always fill it. Leaving them unwritten leaves whatever the
/// mask puts there, which a decoder reads as data.
fn remainder_bits(version: usize) -> usize {
    match version {
        1 => 0,
        2..=6 => 7,
        _ => 0,
    }
}

/// One encoded symbol, in the shape the front end draws.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Symbol {
    pub version: usize,
    /// Modules per side, `17 + 4 × version`.
    pub size: usize,
    /// One string per row, `1` for a dark module.
    ///
    /// Strings rather than nested arrays of booleans: a version 10 symbol is
    /// 57×57, and 3,249 JSON `true`/`false` tokens is forty times the bytes for
    /// the same picture. The front end reads them character by character.
    pub rows: Vec<String>,
    /// The text that was encoded, echoed so the caller can show it under the
    /// picture without having to keep the two in step itself.
    pub text: String,
}

// ------------------------------------------------------------- GF(256)

/// Log and antilog tables for the field QR arithmetic happens in.
///
/// Built once rather than transcribed: the generator is 2 and the primitive
/// polynomial is 0x11D, and every value follows from those two facts.
struct Field {
    log: [u8; 256],
    exp: [u8; 512],
}

impl Field {
    fn new() -> Self {
        let mut log = [0u8; 256];
        let mut exp = [0u8; 512];
        let mut x: u16 = 1;
        for (i, slot) in exp.iter_mut().enumerate().take(255) {
            *slot = x as u8;
            log[x as usize] = i as u8;
            x <<= 1;
            if x & 0x100 != 0 {
                x ^= 0x11D;
            }
        }
        // The doubled tail lets a product index be added without a modulo.
        for i in 255..512 {
            exp[i] = exp[i - 255];
        }
        Field { log, exp }
    }

    fn mul(&self, a: u8, b: u8) -> u8 {
        if a == 0 || b == 0 {
            return 0;
        }
        self.exp[self.log[a as usize] as usize + self.log[b as usize] as usize]
    }
}

/// The generator polynomial for `degree` error-correction codewords.
fn generator(field: &Field, degree: usize) -> Vec<u8> {
    let mut poly = vec![1u8];
    for i in 0..degree {
        // Multiply by (x - α^i).
        let mut next = vec![0u8; poly.len() + 1];
        for (j, coefficient) in poly.iter().enumerate() {
            next[j] ^= *coefficient;
            next[j + 1] ^= field.mul(*coefficient, field.exp[i]);
        }
        poly = next;
    }
    poly
}

/// Reed-Solomon remainder for one block.
fn ecc(field: &Field, data: &[u8], degree: usize) -> Vec<u8> {
    let gen = generator(field, degree);
    let mut remainder = vec![0u8; degree];
    for byte in data {
        let factor = byte ^ remainder[0];
        remainder.remove(0);
        remainder.push(0);
        for (i, g) in gen.iter().skip(1).enumerate() {
            remainder[i] ^= field.mul(*g, factor);
        }
    }
    remainder
}

// ------------------------------------------------------------- bit stream

#[derive(Default)]
struct Bits {
    bytes: Vec<u8>,
    len: usize,
}

impl Bits {
    fn push(&mut self, value: u32, width: usize) {
        for i in (0..width).rev() {
            let bit = (value >> i) & 1 == 1;
            if self.len % 8 == 0 {
                self.bytes.push(0);
            }
            if bit {
                let index = self.len / 8;
                self.bytes[index] |= 0x80 >> (self.len % 8);
            }
            self.len += 1;
        }
    }
}

// ------------------------------------------------------------- encoding

/// Which version holds this many bytes at level M, if any of the ten do.
fn version_for(len: usize) -> Option<usize> {
    (1..=MAX_VERSION).find(|&version| {
        let (_, b1, d1, b2, d2) = BLOCKS_M[version - 1];
        let capacity = b1 * d1 + b2 * d2;
        // Mode indicator plus the character count field, which widens at 10.
        let count_bits = if version < 10 { 8 } else { 16 };
        let needed = (4 + count_bits + len * 8).div_ceil(8);
        needed <= capacity
    })
}

/// The data codewords: mode, length, payload, terminator and padding.
fn codewords(text: &str, version: usize) -> Vec<u8> {
    let (_, b1, d1, b2, d2) = BLOCKS_M[version - 1];
    let capacity = b1 * d1 + b2 * d2;

    let mut bits = Bits::default();
    bits.push(0b0100, 4); // byte mode
    bits.push(text.len() as u32, if version < 10 { 8 } else { 16 });
    for byte in text.as_bytes() {
        bits.push(*byte as u32, 8);
    }

    // Terminator: up to four zero bits, fewer if the capacity ends first.
    let free = capacity * 8 - bits.len;
    bits.push(0, free.min(4));
    // Then to a byte boundary.
    let stray = bits.len % 8;
    if stray != 0 {
        bits.push(0, 8 - stray);
    }

    let mut out = bits.bytes;
    // The two pad bytes the specification names, alternating.
    for i in 0..(capacity - out.len()) {
        out.push(if i % 2 == 0 { 0xEC } else { 0x11 });
    }
    out
}

/// Split into blocks, add error correction, and interleave both.
fn interleave(field: &Field, data: &[u8], version: usize) -> Vec<u8> {
    let (ecc_len, b1, d1, b2, d2) = BLOCKS_M[version - 1];

    let mut blocks: Vec<&[u8]> = Vec::new();
    let mut at = 0;
    for _ in 0..b1 {
        blocks.push(&data[at..at + d1]);
        at += d1;
    }
    for _ in 0..b2 {
        blocks.push(&data[at..at + d2]);
        at += d2;
    }
    let parities: Vec<Vec<u8>> = blocks.iter().map(|b| ecc(field, b, ecc_len)).collect();

    let mut out = Vec::with_capacity(TOTAL_CODEWORDS[version - 1]);
    let widest = d1.max(d2);
    for i in 0..widest {
        for block in &blocks {
            if let Some(byte) = block.get(i) {
                out.push(*byte);
            }
        }
    }
    for i in 0..ecc_len {
        for parity in &parities {
            out.push(parity[i]);
        }
    }
    out
}

// ------------------------------------------------------------- the matrix

/// A module and whether anything may still be written over it.
#[derive(Clone, Copy, PartialEq)]
enum Cell {
    /// Function pattern or format area: fixed, and never masked.
    Fixed(bool),
    Data(bool),
    Empty,
}

struct Matrix {
    size: usize,
    cells: Vec<Cell>,
}

impl Matrix {
    fn new(size: usize) -> Self {
        Matrix {
            size,
            cells: vec![Cell::Empty; size * size],
        }
    }

    fn at(&self, row: usize, col: usize) -> Cell {
        self.cells[row * self.size + col]
    }

    fn set(&mut self, row: usize, col: usize, cell: Cell) {
        let size = self.size;
        self.cells[row * size + col] = cell;
    }

    fn dark(&self, row: usize, col: usize) -> bool {
        match self.at(row, col) {
            Cell::Fixed(v) | Cell::Data(v) => v,
            Cell::Empty => false,
        }
    }

    fn finder(&mut self, row: usize, col: usize) {
        for dr in 0..7 {
            for dc in 0..7 {
                let edge = dr == 0 || dr == 6 || dc == 0 || dc == 6;
                let core = (2..=4).contains(&dr) && (2..=4).contains(&dc);
                self.set(row + dr, col + dc, Cell::Fixed(edge || core));
            }
        }
    }

    /// The one-module light border each finder needs to be recognisable.
    ///
    /// All three at once, because they are three different geometries and
    /// writing one function that takes a corner produced a border running off
    /// into the data region — which laid eight fewer bits and decoded as
    /// nothing.
    fn separators(&mut self) {
        let size = self.size;
        for i in 0..8 {
            self.set(7, i, Cell::Fixed(false));
            self.set(i, 7, Cell::Fixed(false));
            self.set(7, size - 1 - i, Cell::Fixed(false));
            self.set(i, size - 8, Cell::Fixed(false));
            self.set(size - 8, i, Cell::Fixed(false));
            self.set(size - 1 - i, 7, Cell::Fixed(false));
        }
    }

    fn alignment(&mut self, row: usize, col: usize) {
        for dr in 0..5 {
            for dc in 0..5 {
                let edge = dr == 0 || dr == 4 || dc == 0 || dc == 4;
                let centre = dr == 2 && dc == 2;
                self.set(row + dr - 2, col + dc - 2, Cell::Fixed(edge || centre));
            }
        }
    }
}

/// Everything that is the same whatever the data says.
fn function_patterns(matrix: &mut Matrix, version: usize) {
    let size = matrix.size;

    matrix.finder(0, 0);
    matrix.finder(0, size - 7);
    matrix.finder(size - 7, 0);
    matrix.separators();

    // Timing patterns, on row and column 6, alternating from the finders.
    for i in 8..size - 8 {
        let dark = i % 2 == 0;
        matrix.set(6, i, Cell::Fixed(dark));
        matrix.set(i, 6, Cell::Fixed(dark));
    }

    // Alignment patterns, at every pair of centres that does not sit on a
    // finder — the three corners are already patterned and overwriting one
    // there is how a symbol becomes unreadable in a way nothing reports.
    let centres = ALIGNMENT[version - 1];
    for &row in centres {
        for &col in centres {
            // The three patterned corners. Written as "near an edge on both
            // axes, but not the bottom right" rather than three clauses,
            // because the fourth corner is the one that HAS an alignment
            // pattern and the shape should say so.
            let top = row < 8;
            let left = col < 8;
            let bottom = row >= size - 8;
            let right = col >= size - 8;
            let on_finder = (top && (left || right)) || (bottom && left);
            if !on_finder {
                matrix.alignment(row, col);
            }
        }
    }

    // The module that is always dark.
    matrix.set(size - 8, 8, Cell::Fixed(true));

    // Reserve the format areas so data placement walks past them.
    for i in 0..9 {
        if matrix.at(8, i) == Cell::Empty {
            matrix.set(8, i, Cell::Fixed(false));
        }
        if matrix.at(i, 8) == Cell::Empty {
            matrix.set(i, 8, Cell::Fixed(false));
        }
    }
    for i in 0..8 {
        if matrix.at(8, size - 1 - i) == Cell::Empty {
            matrix.set(8, size - 1 - i, Cell::Fixed(false));
        }
        if matrix.at(size - 1 - i, 8) == Cell::Empty {
            matrix.set(size - 1 - i, 8, Cell::Fixed(false));
        }
    }

    // Version information, from 7 up.
    if version >= 7 {
        let bits = version_bits(version);
        for i in 0..18 {
            let bit = (bits >> i) & 1 == 1;
            let (a, b) = (i / 3, i % 3);
            matrix.set(size - 11 + b, a, Cell::Fixed(bit));
            matrix.set(a, size - 11 + b, Cell::Fixed(bit));
        }
    }
}

/// The 18-bit version string: six data bits and a BCH(18,6) remainder.
fn version_bits(version: usize) -> u32 {
    let version = version as u32;
    let mut remainder = version;
    for _ in 0..12 {
        remainder = (remainder << 1) ^ ((remainder >> 11) * 0x1F25);
    }
    (version << 12) | remainder
}

/// The 15-bit format string for level M and one mask.
fn format_bits(mask: usize) -> u32 {
    // Level M is `00`, so the five data bits ARE the mask number. Written as
    // the two fields rather than as `mask` alone, because the day a second
    // level is offered this is the line that changes.
    const LEVEL_M: u32 = 0b00;
    let data = (LEVEL_M << 3) | mask as u32;
    let mut remainder = data;
    for _ in 0..10 {
        remainder = (remainder << 1) ^ ((remainder >> 9) * 0x537);
    }
    ((data << 10) | remainder) ^ 0x5412
}

fn write_format(matrix: &mut Matrix, mask: usize) {
    let bits = format_bits(mask);
    let size = matrix.size;
    let bit = |i: usize| (bits >> i) & 1 == 1;

    // First copy, wrapped around the top-left finder: the low bits run DOWN
    // column 8 and the high ones run left along row 8. Writing it the other way
    // round — bits 0..5 along row 8 — produces a symbol whose finders, timing
    // and data are all correct and which no decoder will read, because the
    // format is the first thing read and it decides the mask. Measured: macOS's
    // own decoder returned nothing at all for every one of them.
    for i in 0..6 {
        matrix.set(i, 8, Cell::Fixed(bit(i)));
    }
    matrix.set(7, 8, Cell::Fixed(bit(6)));
    matrix.set(8, 8, Cell::Fixed(bit(7)));
    matrix.set(8, 7, Cell::Fixed(bit(8)));
    for i in 9..15 {
        matrix.set(8, 14 - i, Cell::Fixed(bit(i)));
    }

    // Second copy, split between the other two finders: bits 0..7 along row 8
    // from the right edge, then 8..14 down column 8 from the bottom finder.
    for i in 0..8 {
        matrix.set(8, size - 1 - i, Cell::Fixed(bit(i)));
    }
    for i in 8..15 {
        matrix.set(size - 15 + i, 8, Cell::Fixed(bit(i)));
    }
}

/// Lay the codewords into the symbol, two columns at a time, right to left.
fn place(matrix: &mut Matrix, data: &[u8], remainder: usize) {
    let size = matrix.size;
    let total = data.len() * 8 + remainder;
    let mut written = 0;
    let mut upward = true;

    let mut col = size as isize - 1;
    while col > 0 {
        // Column 6 is the vertical timing pattern; the pair steps left over it.
        if col == 6 {
            col -= 1;
        }
        for step in 0..size {
            let row = if upward { size - 1 - step } else { step };
            for offset in 0..2 {
                let c = (col - offset) as usize;
                if matrix.at(row, c) != Cell::Empty {
                    continue;
                }
                let bit = if written < data.len() * 8 {
                    data[written / 8] >> (7 - written % 8) & 1 == 1
                } else {
                    // Remainder bits are written as light rather than left
                    // empty: what a mask does to an unwritten module is what a
                    // decoder reads as data.
                    false
                };
                matrix.set(row, c, Cell::Data(bit));
                written += 1;
                if written == total {
                    return;
                }
            }
        }
        upward = !upward;
        col -= 2;
    }
}

fn mask_at(mask: usize, row: usize, col: usize) -> bool {
    let (r, c) = (row, col);
    match mask {
        0 => (r + c) % 2 == 0,
        1 => r % 2 == 0,
        2 => c % 3 == 0,
        3 => (r + c) % 3 == 0,
        4 => (r / 2 + c / 3) % 2 == 0,
        5 => (r * c) % 2 + (r * c) % 3 == 0,
        6 => ((r * c) % 2 + (r * c) % 3) % 2 == 0,
        _ => ((r + c) % 2 + (r * c) % 3) % 2 == 0,
    }
}

fn apply_mask(matrix: &Matrix, mask: usize) -> Matrix {
    let mut out = Matrix::new(matrix.size);
    for row in 0..matrix.size {
        for col in 0..matrix.size {
            let cell = match matrix.at(row, col) {
                Cell::Data(v) => Cell::Data(v ^ mask_at(mask, row, col)),
                other => other,
            };
            out.set(row, col, cell);
        }
    }
    out
}

/// The four penalty rules, which pick the mask that is least likely to confuse
/// a decoder — a symbol with a finder-like run in the middle of its data is
/// still valid and still fails to scan.
fn penalty(matrix: &Matrix) -> usize {
    let size = matrix.size;
    let mut score = 0;

    // Rule 1: runs of five or more.
    for i in 0..size {
        for horizontal in [true, false] {
            let mut run = 1;
            for j in 1..size {
                let (a, b) = if horizontal {
                    (matrix.dark(i, j), matrix.dark(i, j - 1))
                } else {
                    (matrix.dark(j, i), matrix.dark(j - 1, i))
                };
                if a == b {
                    run += 1;
                } else {
                    if run >= 5 {
                        score += 3 + (run - 5);
                    }
                    run = 1;
                }
            }
            if run >= 5 {
                score += 3 + (run - 5);
            }
        }
    }

    // Rule 2: every 2×2 block of one colour.
    for row in 0..size - 1 {
        for col in 0..size - 1 {
            let first = matrix.dark(row, col);
            if matrix.dark(row, col + 1) == first
                && matrix.dark(row + 1, col) == first
                && matrix.dark(row + 1, col + 1) == first
            {
                score += 3;
            }
        }
    }

    // Rule 3: the finder-like sequence, either way round.
    const A: [bool; 11] = [
        true, false, true, true, true, false, true, false, false, false, false,
    ];
    const B: [bool; 11] = [
        false, false, false, false, true, false, true, true, true, false, true,
    ];
    for i in 0..size {
        for j in 0..size.saturating_sub(10) {
            for horizontal in [true, false] {
                let window: Vec<bool> = (0..11)
                    .map(|k| {
                        if horizontal {
                            matrix.dark(i, j + k)
                        } else {
                            matrix.dark(j + k, i)
                        }
                    })
                    .collect();
                if window == A || window == B {
                    score += 40;
                }
            }
        }
    }

    // Rule 4: how far the dark proportion is from half.
    let dark = (0..size)
        .flat_map(|r| (0..size).map(move |c| (r, c)))
        .filter(|(r, c)| matrix.dark(*r, *c))
        .count();
    let percent = dark * 100 / (size * size);
    let deviation = percent.abs_diff(50);
    score += (deviation / 5) * 10;

    score
}

// ------------------------------------------------------------- the surface

/// Encode `text` as a level-M QR symbol.
///
/// Errors rather than truncating: a QR code that holds part of a URL is
/// indistinguishable from one that holds all of it until somebody scans it.
pub fn encode(text: &str) -> Result<Symbol> {
    if text.is_empty() {
        return Err(Error::new(
            Code::InvalidInput,
            "there is nothing to put in a QR code",
        ));
    }
    let Some(version) = version_for(text.len()) else {
        return Err(Error::new(
            Code::InvalidInput,
            format!(
                "{} bytes is more than a version {MAX_VERSION} QR code holds",
                text.len()
            ),
        ));
    };

    let field = Field::new();
    let data = interleave(&field, &codewords(text, version), version);

    let size = 17 + 4 * version;
    let mut base = Matrix::new(size);
    function_patterns(&mut base, version);
    place(&mut base, &data, remainder_bits(version));

    // Every mask is drawn and scored, and the best one wins. Picking one
    // without scoring produces symbols that are valid and do not scan.
    let mut best: Option<(usize, Matrix)> = None;
    for mask in 0..8 {
        let mut candidate = apply_mask(&base, mask);
        write_format(&mut candidate, mask);
        let score = penalty(&candidate);
        if best.as_ref().is_none_or(|(top, _)| score < *top) {
            best = Some((score, candidate));
        }
    }
    let (_, chosen) = best.expect("eight masks were scored");

    let rows = (0..size)
        .map(|row| {
            (0..size)
                .map(|col| if chosen.dark(row, col) { '1' } else { '0' })
                .collect()
        })
        .collect();

    Ok(Symbol {
        version,
        size,
        rows,
        text: text.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The transcribed table, checked against the derived one. Every figure in
    /// `BLOCKS_M` was typed in by hand, and a single wrong digit produces a
    /// symbol that encodes cleanly and decodes to nothing.
    #[test]
    fn the_block_table_agrees_with_the_codeword_counts() {
        for version in 1..=MAX_VERSION {
            let (ecc_len, b1, d1, b2, d2) = BLOCKS_M[version - 1];
            let total = b1 * (d1 + ecc_len) + b2 * (d2 + ecc_len);
            assert_eq!(
                total,
                TOTAL_CODEWORDS[version - 1],
                "version {version} does not add up"
            );
        }
    }

    /// ISO/IEC 18004's own worked example: `01234567` in version 1-M encodes to
    /// a known set of error correction codewords. It is given in numeric mode
    /// there; what is reused here is the arithmetic, driven with the byte-mode
    /// codewords the specification lists for the same version.
    #[test]
    fn reed_solomon_matches_the_published_example() {
        let field = Field::new();
        // The 16 data codewords of the standard's version 1-M example.
        let data = [
            0x10, 0x20, 0x0C, 0x56, 0x61, 0x80, 0xEC, 0x11, 0xEC, 0x11, 0xEC, 0x11, 0xEC, 0x11,
            0xEC, 0x11,
        ];
        let parity = ecc(&field, &data, 10);
        assert_eq!(
            parity,
            [0xA5, 0x24, 0xD4, 0xC1, 0xED, 0x36, 0xC7, 0x87, 0x2C, 0x55]
        );
    }

    /// The field is generated, not transcribed, so this asserts the two facts
    /// it is generated from rather than the 256 values that follow.
    #[test]
    fn the_field_is_the_one_qr_uses() {
        let field = Field::new();
        assert_eq!(field.exp[0], 1);
        assert_eq!(field.exp[8], 0x1D, "0x11D is the primitive polynomial");
        assert_eq!(field.mul(0, 200), 0);
        // α^255 = 1: the multiplicative group has order 255.
        assert_eq!(field.exp[255], 1);
    }

    /// The format string for mask 0 at level M is fixed by the specification,
    /// and a wrong one makes every symbol unreadable however good the data is.
    #[test]
    fn format_strings_are_the_published_ones() {
        assert_eq!(format_bits(0), 0b101010000010010);
        assert_eq!(format_bits(4), 0b100010111111001);
        assert_eq!(format_bits(7), 0b100101010100000);
    }

    /// Version 7's string is the one worked through in the specification.
    #[test]
    fn version_strings_are_the_published_ones() {
        assert_eq!(version_bits(7), 0b000111110010010100);
        assert_eq!(version_bits(10), 0b001010010011010011);
    }

    #[test]
    fn a_version_is_the_smallest_that_fits() {
        assert_eq!(version_for(1), Some(1));
        // 16 data codewords at version 1, minus the mode and the count byte.
        assert_eq!(version_for(14), Some(1));
        assert_eq!(version_for(15), Some(2));
        assert_eq!(version_for(213), Some(10));
        assert_eq!(version_for(214), None);
    }

    /// Everything about the frame that a decoder looks for before it looks at
    /// any data: the three finders, the timing patterns and the dark module.
    #[test]
    fn the_frame_is_where_a_decoder_looks_for_it() {
        let symbol = encode("https://shop.192-168-1-5.sslip.io").unwrap();
        let size = symbol.size;
        let dark = |row: usize, col: usize| symbol.rows[row].as_bytes()[col] == b'1';

        for (row, col) in [(0, 0), (0, size - 7), (size - 7, 0)] {
            // The ring, and the light square inside it.
            assert!(dark(row, col));
            assert!(dark(row + 6, col + 6));
            assert!(!dark(row + 1, col + 1));
            assert!(dark(row + 3, col + 3));
        }

        // Timing: alternating, starting dark at module 6.
        for i in 8..size - 8 {
            assert_eq!(dark(6, i), i % 2 == 0, "timing row at {i}");
            assert_eq!(dark(i, 6), i % 2 == 0, "timing column at {i}");
        }

        assert!(dark(size - 8, 8), "the always-dark module is light");
    }

    /// A symbol is square, of the size its version says, and every row is that
    /// long — the front end indexes it by position and a short row would draw a
    /// picture that is subtly not the code.
    #[test]
    fn the_matrix_is_square_and_complete() {
        for text in ["a", "https://example.com", &"x".repeat(200)] {
            let symbol = encode(text).unwrap();
            assert_eq!(symbol.size, 17 + 4 * symbol.version);
            assert_eq!(symbol.rows.len(), symbol.size);
            for row in &symbol.rows {
                assert_eq!(row.len(), symbol.size);
                assert!(row.bytes().all(|b| b"01".contains(&b)));
            }
        }
    }

    /// Nothing is left unwritten. An empty module inside the data region is
    /// light in the picture and is read as a zero bit, which is a silent
    /// corruption rather than a failure.
    #[test]
    fn every_module_is_written() {
        for version in 1..=MAX_VERSION {
            let size = 17 + 4 * version;
            let mut matrix = Matrix::new(size);
            function_patterns(&mut matrix, version);
            let field = Field::new();
            let data = interleave(&field, &codewords("x", version), version);
            place(&mut matrix, &data, remainder_bits(version));
            assert!(
                !matrix.cells.contains(&Cell::Empty),
                "version {version} has unwritten modules"
            );
        }
    }

    /// The count field widens at version 10, and a symbol encoded with the
    /// narrow one there decodes as garbage.
    #[test]
    fn the_character_count_widens_at_version_ten() {
        let symbol = encode(&"x".repeat(200)).unwrap();
        assert_eq!(symbol.version, 10);
        let data = codewords(&"x".repeat(200), 10);
        // 0100 mode, then sixteen bits of length: 0000 0000 1100 1000 = 200.
        // The count straddles the byte boundary — which is the whole reason
        // this is worth a test rather than an eyeball.
        assert_eq!(data[0], 0b0100_0000);
        assert_eq!(data[1], 0b0000_1100);
        assert_eq!(data[2], 0b1000_0111, "the low nibble is the first 'x'");
    }

    /// Read the format string back the way a decoder does — both copies — and
    /// insist they agree with each other and with one of the eight masks.
    ///
    /// This is the regression test for the one bug this module actually had:
    /// the first copy was written along row 8 instead of down column 8, which
    /// is the transpose of the right answer. Every other test passed. The
    /// finders were right, the timing was right, the codewords were right, and
    /// macOS read nothing at all from any of them, because the format is the
    /// first thing read and it names the mask.
    #[test]
    fn both_format_copies_say_the_same_thing() {
        for text in ["a", "https://shop.loc", &"x".repeat(213)] {
            let symbol = encode(text).unwrap();
            let size = symbol.size;
            let dark = |row: usize, col: usize| symbol.rows[row].as_bytes()[col] == b'1';

            let mut first = 0u32;
            let mut second = 0u32;
            let set = |target: &mut u32, i: usize, on: bool| {
                if on {
                    *target |= 1 << i;
                }
            };

            for i in 0..6 {
                set(&mut first, i, dark(i, 8));
            }
            set(&mut first, 6, dark(7, 8));
            set(&mut first, 7, dark(8, 8));
            set(&mut first, 8, dark(8, 7));
            for i in 9..15 {
                set(&mut first, i, dark(8, 14 - i));
            }

            for i in 0..8 {
                set(&mut second, i, dark(8, size - 1 - i));
            }
            for i in 8..15 {
                set(&mut second, i, dark(size - 15 + i, 8));
            }

            assert_eq!(first, second, "the two format copies disagree for {text:?}");
            let mask = (0..8).find(|m| format_bits(*m) == first);
            assert!(
                mask.is_some(),
                "the format string is not one of the eight for {text:?}"
            );
            // The three level bits, after the specification's own XOR: 00 is M.
            assert_eq!((first ^ 0x5412) >> 13, 0b00, "the level is not M");
        }
    }

    #[test]
    fn nothing_is_encoded_silently_short() {
        assert!(encode("").is_err());
        assert!(encode(&"x".repeat(214)).is_err());
        assert!(encode(&"x".repeat(213)).is_ok());
    }
}
