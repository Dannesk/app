use iced::window;

/// Wayland has no window icon (winit ignores it there): the shell finds
/// `dannesk.desktop` by this app_id and draws that file's `Icon=`. On X11 it
/// is WM_CLASS too. Must match the .deb's desktop file name.
pub const APP_ID: &str = "dannesk";

/// X11 draws these pixels in the taskbar and title bar.
pub fn window_icon() -> Option<window::Icon> {
    let img = image::load_from_memory(include_bytes!("128x128.png")).ok()?.to_rgba8();
    let (width, height) = img.dimensions();
    window::icon::from_rgba(img.into_raw(), width, height).ok()
}
