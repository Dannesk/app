/// Adds thousand separators (commas) to an i64 number for display.
/// Handles negatives by prefixing a minus sign.
/// Assumes positive balances typically, but works for negatives.
pub fn add_commas(num: i64) -> String {
    if num < 0 {
        format!("-{}", add_commas(-num))
    } else {
        let mut s = String::new();
        let digits = num.to_string();
        // Dropped unused `len`—not needed for the loop logic
        for (i, c) in digits.chars().rev().enumerate() {
            if i > 0 && i % 3 == 0 {
                s.push(',');
            }
            s.push(c);
        }
        s.chars().rev().collect()
    }
}

/// An issued-currency amount as the ledger wrote it. Rust's `{}` for an f64
/// is the shortest decimal that parses back to the same value, never in
/// exponent form; a ledger value has at most 15 significant digits, which an
/// f64 carries exactly, so the string that came off the wire comes back.
/// Zero is `"0"`, whole numbers have no point. This is what gets SIGNED.
pub fn exact_amount(v: f64) -> String {
    let s = format!("{}", v);
    s.strip_suffix(".0").map(str::to_string).unwrap_or(s)
}

pub fn format_token_amount(val: f64, decimals: usize) -> String {
    let s = format!("{:.1$}", val, decimals);
    if let Some(dot_idx) = s.find('.') {
        let int = &s[..dot_idx];
        let frac = &s[dot_idx + 1..].trim_end_matches('0');
        if frac.is_empty() {
            format!("{}.00", int)
        } else if frac.len() == 1 {
            format!("{}.{}0", int, frac)
        } else {
            format!("{}.{}", int, frac)
        }
    } else {
        format!("{}.00", s)
    }
}

/// Thousands-separated to 2 dp. Rounds to cents *first* so a value like
/// `64_989.999` carries into the integer part instead of rendering `64,989.100`.
pub fn money(v: f64) -> String {
    let cents = (v * 100.0).round() as i64;
    format!("{}.{:02}", add_commas(cents / 100), (cents % 100).abs())
}

pub fn format_usd(val: f64) -> String {
    format_token_amount(val, 4)
}

/// A fiat amount derived from a crypto amount at a rate — the send form's
/// fiat twin and the review total. Two places, as money has, extended to as
/// many as four while the value is small enough that cents would hide what
/// the rate shows: `1 XRP` at `1.3069` reads `1.3069`, as the chart beside it
/// does, not `1.31`; `100 XRP` reads `130.69`; a thousand and up stays at
/// cents. Five significant figures — the chart's own precision for a rate.
///
/// It used to be a flat two, which put `1.31` in the field next to a chart
/// saying `1.3069` — the two numbers disagreeing is what a user sees, not the
/// convention behind it. A flat four was rejected before that: it rendered
/// `17881.2003` and pushed two meaningless digits past the edge of the field,
/// which is why the count is tied to the magnitude. Rounding to nearest at
/// the last place shown is what every finite display does, the chart
/// included; the point is that they agree.
///
/// [`format_usd`] keeps a flat four for BTC fees, which are routinely sub-cent
/// (`47 sats ≈ 0.0377 USD`). Nothing is lost by rounding here: the fiat figure
/// is a convenience for deciding an amount, and the transaction is always
/// built from the crypto field beside it.
pub fn fiat_amount(val: f64) -> String {
    let int_digits = (val.abs().trunc() as u64).to_string().len();
    let decimals = 5usize.saturating_sub(int_digits).clamp(2, 4);
    format_token_amount(val, decimals)
}

#[cfg(test)]
mod tests {
    use super::fiat_amount;

    #[test]
    fn a_fiat_amount_carries_the_rate_until_cents_are_the_noise_floor() {
        assert_eq!(fiat_amount(1.3069), "1.3069");
        assert_eq!(fiat_amount(13.069), "13.069");
        assert_eq!(fiat_amount(130.69), "130.69");
        assert_eq!(fiat_amount(1306.9), "1306.90");
        assert_eq!(fiat_amount(16254.87), "16254.87");
        assert_eq!(fiat_amount(0.0377), "0.0377");
        assert_eq!(fiat_amount(1.3), "1.30");
        assert_eq!(fiat_amount(0.0), "0.00");
    }
}
