//! Can anything actually read what the encoder drew? (M-3)
//!
//! `qr.rs` is checked in its unit tests against the specification's own worked
//! example and against the format and version strings it publishes. That is the
//! arithmetic. It says nothing about the picture: a symbol can have correct
//! codewords, correct error correction and correct format bits and still be
//! unreadable because a separator ran one module too far or the mask was
//! applied to a function pattern.
//!
//! There is exactly one honest way to settle that, and it is not another
//! assertion written by the same author. **macOS ships a QR decoder** —
//! `CIDetector`, the one the camera uses — and this hands it the encoder's
//! output and asks what it reads:
//!
//! ```sh
//! cargo run --example qr_probe
//! ```
//!
//! Each case is encoded, written out as a bitmap in a scratch directory, and
//! decoded by `osascript` through the Objective-C bridge. A row passes only
//! when the decoded text is **byte-identical** to what went in. Nothing on this
//! machine is read or changed, and the scratch directory is removed on the way
//! out.
//!
//! On anything that is not macOS the decoder is absent and the probe says so
//! rather than passing quietly — a measurement that cannot run is not a
//! measurement that succeeded.

use stackvo_desktop_lib::qr;
use std::path::Path;
use std::process::Command;

fn main() {
    let scratch = std::env::temp_dir().join(format!("stackvo-qr-probe-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    if std::fs::create_dir_all(&scratch).is_err() {
        println!("could not make a scratch directory");
        return;
    }

    if !cfg!(target_os = "macos") {
        println!("this probe needs macOS's own decoder; nothing was measured here.");
        return;
    }

    // Every shape this app can hand somebody, plus the edges of the encoder.
    let cases = [
        "https://shop.loc",
        "https://shop.192-168-1-5.sslip.io",
        "https://fine-shop-plenty-mars.trycloudflare.com",
        "http://api.loc:8080/callback?state=abc123&code=xyz",
        // Non-ASCII, which is where a byte-mode encoder that counted
        // characters instead of bytes falls over.
        "https://çay.loc/menü",
        // The bottom and the top of the range this draws.
        "a",
        &"x".repeat(213),
    ];

    let mut failures = 0;
    for (i, text) in cases.iter().enumerate() {
        let symbol = match qr::encode(text) {
            Ok(symbol) => symbol,
            Err(e) => {
                failures += 1;
                println!("  FAIL {:<46} did not encode: {}", shorten(text), e.message);
                continue;
            }
        };

        let path = scratch.join(format!("case-{i}.bmp"));
        if let Err(e) = write_bitmap(&path, &symbol) {
            failures += 1;
            println!("  FAIL {:<46} could not be written: {e}", shorten(text));
            continue;
        }

        let read = decode(&path);
        let ok = read.as_deref() == Some(*text);
        if !ok {
            failures += 1;
        }
        println!(
            "  {} {:<46} v{:<2} {}×{}  →  {}",
            if ok { "ok  " } else { "FAIL" },
            shorten(text),
            symbol.version,
            symbol.size,
            symbol.size,
            match &read {
                Some(t) if ok => format!("read back {} bytes, identical", t.len()),
                Some(t) => format!("read back {:?}", shorten(t)),
                None => "nothing was read".to_string(),
            }
        );
    }

    println!();
    if failures == 0 {
        println!("macOS read every symbol back as the text that went in.");
        let _ = std::fs::remove_dir_all(&scratch);
    } else {
        println!("{failures} symbol(s) did not come back; the bitmaps are in {scratch:?}");
    }
}

fn shorten(text: &str) -> String {
    if text.chars().count() <= 44 {
        return text.to_string();
    }
    let head: String = text.chars().take(41).collect();
    format!("{head}...")
}

/// A 24-bit bitmap, written by hand.
///
/// BMP rather than PNG because PNG means a deflate stream and a CRC, which is
/// a compression library pulled in so a probe can draw squares. ImageIO reads
/// BMP, and every byte of this header is fixed.
///
/// The quiet zone is four modules on every side. It is part of the symbol, not
/// decoration: a decoder that cannot find four light modules around the finders
/// does not recognise the pattern, and a code drawn hard against an edge is the
/// most common reason a valid symbol will not scan.
fn write_bitmap(path: &Path, symbol: &qr::Symbol) -> std::io::Result<()> {
    const SCALE: usize = 8;
    const QUIET: usize = 4;

    let modules = symbol.size + QUIET * 2;
    let side = modules * SCALE;
    let stride = (side * 3).div_ceil(4) * 4;
    let pixels = stride * side;

    let mut out = Vec::with_capacity(54 + pixels);
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&((54 + pixels) as u32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&54u32.to_le_bytes());
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(side as i32).to_le_bytes());
    out.extend_from_slice(&(side as i32).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&24u16.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&(pixels as u32).to_le_bytes());
    out.extend_from_slice(&2835i32.to_le_bytes());
    out.extend_from_slice(&2835i32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());

    // Bottom-up, which is what a BMP is; the row index is flipped so the
    // symbol is not written upside down. A QR code decodes upside down too,
    // which is exactly why getting this wrong would go unnoticed.
    for y in 0..side {
        let row = side - 1 - y;
        let mut line = Vec::with_capacity(stride);
        for x in 0..side {
            let mr = row / SCALE;
            let mc = x / SCALE;
            let dark = mr >= QUIET
                && mc >= QUIET
                && mr < QUIET + symbol.size
                && mc < QUIET + symbol.size
                && symbol.rows[mr - QUIET].as_bytes()[mc - QUIET] == b'1';
            let value = if dark { 0u8 } else { 255u8 };
            line.extend_from_slice(&[value, value, value]);
        }
        line.resize(stride, 0);
        out.extend_from_slice(&line);
    }

    std::fs::write(path, out)
}

/// Ask macOS what the picture says.
///
/// `CIDetector` is the same decoder behind the camera's QR handling, reached
/// through JavaScript for Automation so no compiler is needed on the machine
/// running this.
fn decode(path: &Path) -> Option<String> {
    let file = path.display().to_string();
    let script = format!(
        r#"
ObjC.import('CoreImage');
ObjC.import('Foundation');
const url = $.NSURL.fileURLWithPath({file:?});
const image = $.CIImage.imageWithContentsOfURL(url);
if (!image || image.isNil()) {{ throw new Error('no image'); }}
const detector = $.CIDetector.detectorOfTypeContextOptions(
  'CIDetectorTypeQRCode', $(), $({{ CIDetectorAccuracy: 'CIDetectorAccuracyHigh' }})
);
const found = detector.featuresInImage(image);
if (found.count === 0) {{ '' }} else {{ ObjC.unwrap(found.objectAtIndex(0).messageString) }}
"#
    );

    let output = Command::new("osascript")
        .args(["-l", "JavaScript", "-e", &script])
        .output()
        .ok()?;
    if !output.status.success() {
        eprintln!(
            "       the decoder itself failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout)
        .trim_end()
        .to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}
