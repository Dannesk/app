use std::io::Cursor;

use iced::window;

/// Wayland has no window icon (winit ignores it there): the shell finds
/// `dannesk.desktop` by this app_id and draws that file's `Icon=`. On X11 it
/// is WM_CLASS too. Must match the .deb's desktop file name.
pub const APP_ID: &str = "dannesk";

/// X11 draws these pixels in the taskbar and title bar.
///
/// The PNG is decoded with `png`, which iced's tiny-skia renderer already
/// links, so the icon costs no decoder of its own. The decoder is asked for
/// 8-bit RGBA whatever the file's depth or palette; a file it cannot bring to
/// that leaves the window without an icon rather than stopping the start.
pub fn window_icon() -> Option<window::Icon> {
    const PNG: &[u8] = include_bytes!("128x128.png");
    let mut decoder = png::Decoder::new(Cursor::new(PNG));
    decoder.set_transformations(
        png::Transformations::normalize_to_color8() | png::Transformations::ALPHA,
    );
    let mut reader = decoder.read_info().ok()?;
    let mut rgba = vec![0; reader.output_buffer_size()?];
    let frame = reader.next_frame(&mut rgba).ok()?;
    if frame.color_type != png::ColorType::Rgba || frame.bit_depth != png::BitDepth::Eight {
        return None;
    }
    rgba.truncate(frame.buffer_size());
    window::icon::from_rgba(rgba, frame.width, frame.height).ok()
}
