//! QR generation (receive v3, 2026-09-02): the receive pane draws the code
//! as inline SVG — unit rects scaled into whatever box the layout asks for,
//! so the tile lands on the design's exact size no matter how many modules
//! the address needs. `iced::widget::qr_code` cannot — it sizes as
//! `cell_size × modules`, which quantises the tile.
//!
//! One [`QrCode`] at error correction **M**. The `save png ›` file export
//! went with the receive page on 2026-09-10; the pane has copy only.

use std::fmt::Write as FmtWrite;

use iced::Color;
use qrcode::types::Color as QrColor;
use qrcode::{EcLevel, QrCode};

/// The address as an SVG of unit rects on a transparent ground — the tile
/// behind it supplies the white surface and the quiet zone.
///
/// An unrenderable payload yields an empty SVG rather than a panic: the
/// address is still on screen below it, and a missing code is not worth a
/// crash on a screen whose whole job is being shown to someone.
pub fn svg_bytes(address: &str, module: Color) -> Vec<u8> {
    const EMPTY: &[u8] = b"<svg xmlns='http://www.w3.org/2000/svg'/>";
    if address.is_empty() || address == "No Address" {
        return EMPTY.to_vec();
    }
    let Ok(code) = QrCode::with_error_correction_level(address.as_bytes(), EcLevel::M) else {
        return EMPTY.to_vec();
    };
    let w = code.width();
    let mut rects = String::new();
    for y in 0..w {
        for x in 0..w {
            if code[(x, y)] == QrColor::Dark {
                let _ = write!(rects, r#"<rect x="{x}" y="{y}" width="1" height="1"/>"#);
            }
        }
    }
    let fill = hex(module);
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {w}"><g fill="{fill}">{rects}</g></svg>"##
    )
    .into_bytes()
}

fn hex(c: Color) -> String {
    format!(
        "#{:02X}{:02X}{:02X}",
        (c.r * 255.0).round() as u8,
        (c.g * 255.0).round() as u8,
        (c.b * 255.0).round() as u8
    )
}
