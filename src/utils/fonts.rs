
use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use iced::advanced::graphics::text::{self as text, cosmic_text::fontdb};
use iced::{Font, Task, font::{Family, Weight}};

use crate::controller::message::Message;

pub const SANS_FAMILY: &str = "Inter 18pt";

pub const SANS: Font = Font::new(SANS_FAMILY);

pub const MONO: Font = Font {
    family: Family::Name("JetBrains Mono"),
    weight: Weight::Normal,
    stretch: iced::font::Stretch::Normal,
    style: iced::font::Style::Normal,
};

pub const LIGHT: Font = Font {
    family: Family::Name(SANS_FAMILY),
    weight: Weight::Light,
    stretch: iced::font::Stretch::Normal,
    style: iced::font::Style::Normal,
};

static SYSTEM_FONTS_OWED: AtomicBool = AtomicBool::new(false);

fn skip_config() -> PathBuf {
    dirs::runtime_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join(format!("dannesk-fonts-{}.conf", std::process::id()))
}

pub fn index_before_window() {
    let conf = skip_config();
    let nowhere = conf.with_extension("d");
    let config = format!(
        "<?xml version=\"1.0\"?>\n<!DOCTYPE fontconfig SYSTEM \"fonts.dtd\">\n<fontconfig>\n  <dir>{}</dir>\n</fontconfig>\n",
        nowhere.display()
    );
    if std::fs::write(&conf, config).is_err() {
        let _ = text::font_system();
        return;
    }

    unsafe { std::env::set_var("FONTCONFIG_FILE", &conf) };
    let faces = text::font_system().write().expect("Write font system").raw().db().len();

    SYSTEM_FONTS_OWED.store(faces < 16, Ordering::Release);
}

pub fn remove_skip_config() {
    let _ = std::fs::remove_file(skip_config());
}

pub fn system_fonts_pending() -> bool {
    SYSTEM_FONTS_OWED.load(Ordering::Acquire)
}

pub fn system_fonts_task() -> Task<Message> {
    if !SYSTEM_FONTS_OWED.swap(false, Ordering::AcqRel) {
        return Task::none();
    }
    let (done, merged) = tokio::sync::oneshot::channel::<()>();
    let spawned = std::thread::Builder::new().name("system-fonts".into()).spawn(move || {
        index_system_fonts();
        let _ = done.send(());
    });
    if spawned.is_err() {
        return Task::none();
    }
    Task::future(async move {
        let _ = merged.await;
        Message::SystemFontsIndexed
    })
}

fn system_font_dirs() -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = vec!["/usr/share/fonts".into(), "/usr/local/share/fonts".into()];
    let mut add = |dir: PathBuf| {
        if !found.contains(&dir) {
            found.push(dir);
        }
    };
    if let Ok(data_dirs) = std::env::var("XDG_DATA_DIRS") {
        for dir in data_dirs.split(':').filter(|d| !d.is_empty()) {
            add(PathBuf::from(dir).join("fonts"));
        }
    }
    if let Some(dir) = dirs::font_dir() {
        add(dir);
    }
    if let Some(home) = dirs::home_dir() {
        add(home.join(".fonts"));
    }
    found
}

fn index_system_fonts() {
    crate::startup_trace::stamp("system font scan started");
    let mut system = fontdb::Database::new();
    for dir in system_font_dirs() {
        system.load_fonts_dir(dir);
    }

    let mut font_system = text::font_system().write().expect("Write font system");
    let db = font_system.raw().db_mut();
    for face in system.faces() {
        let mut face = face.clone();
        face.id = fontdb::ID::dummy();
        db.push_face_info(face);
    }

    font_system.load_font(Cow::Owned(Vec::new()));
    drop(font_system);
    crate::startup_trace::stamp("system fonts merged");
}
