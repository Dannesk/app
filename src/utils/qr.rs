
use std::fmt::Write as FmtWrite;

use iced::Color;
use qrcode::types::Color as QrColor;
use qrcode::{EcLevel, QrCode};

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

const PNG_MODULE: usize = 8;
const PNG_QUIET: usize = 4;

pub fn save_png(payload: &str, module: Color) -> Option<std::path::PathBuf> {
    let code = QrCode::with_error_correction_level(payload.as_bytes(), EcLevel::M).ok()?;
    let w = code.width();
    let side = (w + 2 * PNG_QUIET) * PNG_MODULE;
    let ink = (255.0 * (0.2126 * module.r + 0.7152 * module.g + 0.0722 * module.b)).round() as u8;
    let mut pixels = vec![255u8; side * side];
    for y in 0..w {
        for x in 0..w {
            if code[(x, y)] != QrColor::Dark {
                continue;
            }
            for dy in 0..PNG_MODULE {
                let start = ((PNG_QUIET + y) * PNG_MODULE + dy) * side + (PNG_QUIET + x) * PNG_MODULE;
                pixels[start..start + PNG_MODULE].fill(ink);
            }
        }
    }
    let folder = dirs::download_dir().filter(|d| d.is_dir()).or_else(dirs::home_dir)?;
    let path = folder.join(format!("dannesk-receive-{}.png", chrono::Local::now().format("%Y%m%d-%H%M%S")));
    let file = std::fs::File::create(&path).ok()?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), side as u32, side as u32);
    encoder.set_color(png::ColorType::Grayscale);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().ok()?;
    writer.write_image_data(&pixels).ok()?;
    writer.finish().ok()?;
    Some(path)
}

fn hex(c: Color) -> String {
    format!(
        "#{:02X}{:02X}{:02X}",
        (c.r * 255.0).round() as u8,
        (c.g * 255.0).round() as u8,
        (c.b * 255.0).round() as u8
    )
}
