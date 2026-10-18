use std::io::Cursor;

use iced::window;


pub const APP_ID: &str = "dannesk";


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
