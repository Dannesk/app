
use crate::channel::CHANNEL;
use crate::utils::tokens::rate_key;

pub fn usd(asset: &str) -> f64 {
    let key = rate_key(asset);
    if key == "USD" {
        return 1.0;
    }
    CHANNEL.rates_rx.borrow().get(key).copied().unwrap_or(0.0) as f64
}

pub fn cross(base: &str, quote: &str) -> f64 {
    let b = usd(base);
    let q = usd(quote);
    if b > 0.0 && q > 0.0 {
        b / q
    } else {
        0.0
    }
}
