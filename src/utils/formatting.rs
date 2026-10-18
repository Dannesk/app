pub fn add_commas(num: i64) -> String {
    if num < 0 {
        format!("-{}", add_commas(-num))
    } else {
        let mut s = String::new();
        let digits = num.to_string();
        for (i, c) in digits.chars().rev().enumerate() {
            if i > 0 && i % 3 == 0 {
                s.push(',');
            }
            s.push(c);
        }
        s.chars().rev().collect()
    }
}

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

pub fn money(v: f64) -> String {
    let cents = (v * 100.0).round() as i64;
    format!("{}.{:02}", add_commas(cents / 100), (cents % 100).abs())
}

pub fn format_usd(val: f64) -> String {
    format_token_amount(val, 4)
}

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
