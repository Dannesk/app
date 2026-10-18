
use iced::Color;

pub const MONO_ADVANCE: f32 = 0.6;

pub type Run = (String, Color);

pub fn n_chars(runs: &[Run]) -> usize {
    runs.iter().map(|(s, _)| s.chars().count()).sum()
}


pub fn pad(s: &str, n: usize) -> String {
    let len = s.chars().count();
    if len >= n { s.to_string() } else { format!("{s}{}", " ".repeat(n - len)) }
}
