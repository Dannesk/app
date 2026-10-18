
use std::sync::Mutex;

#[cfg(target_os = "linux")]
use arboard::SetExtLinux;
use zeroize::Zeroizing;

static CLIPBOARD: Mutex<Option<arboard::Clipboard>> = Mutex::new(None);

pub fn copy_secret(text: String) {
    let text = Zeroizing::new(text);
    let _ = std::thread::Builder::new().name("secret-copy".into()).spawn(move || {
        let Ok(mut slot) = CLIPBOARD.lock() else { return };
        if slot.is_none() {
            *slot = arboard::Clipboard::new().ok();
        }
        let Some(clipboard) = slot.as_mut() else { return };
        let set = clipboard.set();
        #[cfg(target_os = "linux")]
        let set = set.exclude_from_history();
        let _ = set.text(text.as_str());
    });
}
