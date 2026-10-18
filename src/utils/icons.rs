use iced::widget::svg;
use std::sync::LazyLock;

pub static LOGO: LazyLock<svg::Handle> = LazyLock::new(|| {
    svg::Handle::from_memory(include_bytes!("logo.svg").as_ref())
});

pub static EYE_OPEN: LazyLock<svg::Handle> = LazyLock::new(|| {
    svg::Handle::from_memory(include_bytes!("eye-open.svg").as_ref())
});

pub static EYE_CLOSED: LazyLock<svg::Handle> = LazyLock::new(|| {
    svg::Handle::from_memory(include_bytes!("eye-closed.svg").as_ref())
});

pub static SLIDERS: LazyLock<svg::Handle> = LazyLock::new(|| {
    svg::Handle::from_memory(include_bytes!("sliders.svg").as_ref())
});

pub static CHEVRON_LEFT: LazyLock<svg::Handle> = LazyLock::new(|| {
    svg::Handle::from_memory(include_bytes!("chevron-left.svg").as_ref())
});

pub static EMPTY_LIST: LazyLock<svg::Handle> = LazyLock::new(|| {
    svg::Handle::from_memory(include_bytes!("empty-list.svg").as_ref())
});

pub static SEARCH: LazyLock<svg::Handle> = LazyLock::new(|| {
    svg::Handle::from_memory(include_bytes!("search.svg").as_ref())
});

pub static COPY: LazyLock<svg::Handle> = LazyLock::new(|| {
    svg::Handle::from_memory(include_bytes!("copy.svg").as_ref())
});

pub static XRP: LazyLock<svg::Handle> = LazyLock::new(|| {
    svg::Handle::from_memory(include_bytes!("xrp.svg").as_ref())
});
