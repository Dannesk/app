
const ARGON2ID_CORE_SECONDS: f64 = 0.40085;

const BIP39_CORE_SECONDS: f64 = 0.00144;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kdf {
    Argon2id,
    Bip39,
}

impl Kdf {
    fn core_seconds(self) -> f64 {
        match self {
            Kdf::Argon2id => ARGON2ID_CORE_SECONDS,
            Kdf::Bip39 => BIP39_CORE_SECONDS,
        }
    }
}

const USD_PER_CORE_HOUR: f64 = 0.03;

pub const ATTACKER_CORES: f64 = 10_000.0;

const FULL_SCALE_BITS: f64 = 128.0;

const SECONDS_PER_YEAR: f64 = 31_556_952.0;

const UNIVERSE_YEARS: f64 = 13.8e9;

const WORLD_GDP_USD: f64 = 1.1e14;

pub fn bits(secret: &str) -> f64 {
    if secret.is_empty() {
        return 0.0;
    }
    let (mut lower, mut upper, mut digit, mut symbol, mut wide) = (false, false, false, false, false);
    for c in secret.chars() {
        match c {
            'a'..='z' => lower = true,
            'A'..='Z' => upper = true,
            '0'..='9' => digit = true,
            c if c.is_ascii() => symbol = true,
            _ => wide = true,
        }
    }
    let mut pool = 0u32;
    if lower { pool += 26 }
    if upper { pool += 26 }
    if digit { pool += 10 }
    if symbol { pool += 33 }
    if wide { pool += 100 }

    secret.chars().count() as f64 * (pool.max(2) as f64).log2()
}

pub fn bar_fill(bits: f64) -> f32 {
    (bits / FULL_SCALE_BITS).clamp(0.0, 1.0) as f32
}

fn expected_guesses(bits: f64) -> f64 {
    if bits <= 0.0 {
        return 0.0;
    }
    (bits - 1.0).exp2()
}

pub fn crack_cost_usd(bits: f64, kdf: Kdf) -> f64 {
    expected_guesses(bits) * kdf.core_seconds() * (USD_PER_CORE_HOUR / 3600.0)
}

pub fn crack_seconds(bits: f64, kdf: Kdf) -> f64 {
    expected_guesses(bits) * kdf.core_seconds() / ATTACKER_CORES
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tier {
    Weak,
    Fair,
    Strong,
}

pub struct Verdict {
    pub phrase: String,
    pub tier: Tier,
    pub numeric: bool,
}

const HISTORY_YEARS: f64 = 5_000.0;
const HUMANKIND_YEARS: f64 = 300_000.0;
const SUN_YEARS: f64 = 4.6e9;

const STANDARD_BITS: f64 = 128.0;

const ATOMS_NEAR_BITS: f64 = 200.0;
const ATOMS_BITS: f64 = 300.0;

pub const PHRASE_MAX: usize = 40;

pub fn crack_verdict(bits: f64, kdf: Kdf) -> Verdict {
    let secs = crack_seconds(bits, kdf);
    let years = secs / SECONDS_PER_YEAR;
    let (phrase, tier, numeric) = if bits >= ATOMS_BITS {
        ("more than every atom in the universe", Tier::Strong, false)
    } else if bits >= ATOMS_NEAR_BITS {
        ("approaching every atom in the universe", Tier::Strong, false)
    } else if bits >= STANDARD_BITS {
        ("unbreakable with current technology", Tier::Strong, false)
    } else if years >= UNIVERSE_YEARS {
        ("longer than the age of the universe", Tier::Strong, false)
    } else if years >= SUN_YEARS {
        ("longer than the age of the sun", Tier::Fair, false)
    } else if years >= HUMANKIND_YEARS {
        ("more years than humans have existed", Tier::Fair, false)
    } else if years >= HISTORY_YEARS {
        ("longer than recorded history", Tier::Fair, false)
    } else if years >= 1.0 {
        return Verdict { phrase: humanise_time(secs), tier: Tier::Fair, numeric: true };
    } else {
        return Verdict { phrase: humanise_time(secs), tier: Tier::Weak, numeric: true };
    };
    Verdict { phrase: phrase.to_string(), tier, numeric }
}

pub fn crack_cost_phrase(bits: f64, kdf: Kdf) -> String {
    let usd = crack_cost_usd(bits, kdf);
    if usd > WORLD_GDP_USD {
        return "beyond world GDP".to_string();
    }
    humanise_cost(usd)
}

pub fn humanise_time(secs: f64) -> String {
    const MINUTE: f64 = 60.0;
    const HOUR: f64 = 3600.0;
    const DAY: f64 = 86_400.0;
    let years = secs / SECONDS_PER_YEAR;
    match secs {
        s if s < 1.0 => "instantly".to_string(),
        s if s < MINUTE => format!("{} seconds", s as u64),
        s if s < HOUR => format!("{} minutes", (s / MINUTE) as u64),
        s if s < DAY => format!("{} hours", (s / HOUR) as u64),
        s if s < SECONDS_PER_YEAR => format!("{} days", (s / DAY) as u64),
        _ => format!("{} years", magnitude(years)),
    }
}

pub fn humanise_cost(usd: f64) -> String {
    match usd {
        c if c < 0.01 => "under a cent".to_string(),
        c if c < 1.0 => format!("{:.0} cents", c * 100.0),
        c if c < 1_000.0 => format!("${c:.0}"),
        c => format!("${}", magnitude(c)),
    }
}

fn magnitude(n: f64) -> String {
    const SCALES: [(f64, &str); 5] = [
        (1e18, "quintillion"),
        (1e15, "quadrillion"),
        (1e12, "trillion"),
        (1e9, "billion"),
        (1e6, "million"),
    ];
    if n >= 1e21 {
        return "more than counts".to_string();
    }
    for (base, name) in SCALES {
        if n >= base {
            let v = n / base;
            return if v < 10.0 { format!("{v:.1} {name}") } else { format!("{v:.0} {name}") };
        }
    }
    if n >= 1_000.0 {
        let n = n as u64;
        let s = n.to_string();
        let mut out = String::new();
        for (i, c) in s.chars().enumerate() {
            if i > 0 && (s.len() - i) % 3 == 0 {
                out.push(',');
            }
            out.push(c);
        }
        return out;
    }
    format!("{n:.0}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_character_pool_grows_with_the_classes_used() {
        assert_eq!(bits(""), 0.0);
        assert!((bits("123456") - 19.93).abs() < 0.01);
        assert!(bits("aB3$xY") > bits("123456"));
        assert!(bits("aaaaaaaaaaaaaaaaaaaa") > bits("aB3$xY"));
    }

    #[test]
    fn a_million_guesses_costs_three_dollars_and_change() {
        let bits_for_2m = (2.0e6_f64).log2();
        let cost = crack_cost_usd(bits_for_2m, Kdf::Argon2id);
        assert!((cost - 3.34).abs() < 0.01, "million guesses cost ${cost:.2}, expected $3.34");
    }

    #[test]
    fn six_digits_is_pocket_change() {
        let cost = crack_cost_usd(bits("123456"), Kdf::Argon2id);
        assert!(cost < 5.0, "six digits cost ${cost:.2}");
        assert!(crack_seconds(bits("123456"), Kdf::Argon2id) < 60.0, "six digits should fall in under a minute");
    }

    #[test]
    fn a_long_random_key_is_out_of_reach() {
        let strong = bits("cQ7#vN2pLx9!mR4tZ8wK");
        assert!(strong > 100.0, "20 mixed chars scored {strong} bits");
        assert!(crack_cost_usd(strong, Kdf::Argon2id) > 1e15);
    }

    #[test]
    fn every_magnitude_reads_as_words() {
        for b in [0.0, 1.0, 10.0, 20.0, 40.0, 60.0, 80.0, 100.0, 128.0, 200.0, 512.0, 1000.0] {
            for kdf in [Kdf::Argon2id, Kdf::Bip39] {
            let t = humanise_time(crack_seconds(b, kdf));
            let c = humanise_cost(crack_cost_usd(b, kdf));
            for s in [&t, &c] {
                assert!(!s.contains("inf") && !s.contains("NaN"), "{b} bits produced {s:?}");
                assert!(s.len() <= PHRASE_MAX, "{b} bits produced an over-long {s:?}");
            }
            }
        }
        assert_eq!(humanise_time(0.5), "instantly");
        assert_eq!(humanise_cost(0.0), "under a cent");
    }

    #[test]
    fn a_25th_word_is_cheaper_to_guess_than_a_key() {
        let b = bits("hello");
        let ratio = crack_cost_usd(b, Kdf::Argon2id) / crack_cost_usd(b, Kdf::Bip39);
        assert!(ratio > 100.0, "25th word only {ratio:.0}x cheaper than the key");
        assert!(crack_seconds(b, Kdf::Bip39) < 1.0, "\"hello\" as a 25th word should fall instantly");
    }

    #[test]
    fn no_phrase_wears_two_colours() {
        use std::collections::HashMap;
        let mut seen: HashMap<String, Tier> = HashMap::new();
        for kdf in [Kdf::Argon2id, Kdf::Bip39] {
            let mut last = Tier::Weak;
            for tenths in 0..=4000 {
                let b = tenths as f64 / 10.0;
                let v = crack_verdict(b, kdf);
                assert!(v.tier >= last, "{kdf:?} at {b} bits dropped from {last:?} to {:?}", v.tier);
                last = v.tier;
                assert!(v.phrase.chars().count() <= PHRASE_MAX, "{:?} is over the row budget", v.phrase);
                let key = if v.phrase.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                    format!("<{}>", v.phrase.rsplit(' ').next().unwrap())
                } else {
                    v.phrase.clone()
                };
                match seen.insert(key.clone(), v.tier) {
                    Some(t) if t != v.tier => panic!("{key:?} is drawn as both {t:?} and {:?}", v.tier),
                    _ => {}
                }
            }
        }
        for rung in [
            "instantly", "longer than recorded history", "more years than humans have existed",
            "longer than the age of the sun", "longer than the age of the universe",
            "unbreakable with current technology",
            "approaching every atom in the universe", "more than every atom in the universe",
        ] {
            assert!(seen.contains_key(rung), "{rung:?} never appeared");
        }
        assert!(seen.contains_key("<years>") && seen.contains_key("<days>"));
    }

    #[test]
    fn the_tiers_sit_on_a_year_and_on_the_universe() {
        for kdf in [Kdf::Argon2id, Kdf::Bip39] {
            let mut b = 0.0;
            while crack_seconds(b, kdf) < SECONDS_PER_YEAR { b += 0.1; }
            assert_eq!(crack_verdict(b - 0.2, kdf).tier, Tier::Weak);
            assert_eq!(crack_verdict(b + 0.2, kdf).tier, Tier::Fair);
            while crack_seconds(b, kdf) < UNIVERSE_YEARS * SECONDS_PER_YEAR { b += 0.1; }
            assert_eq!(crack_verdict(b - 0.2, kdf).tier, Tier::Fair);
            assert_eq!(crack_verdict(b + 0.2, kdf).tier, Tier::Strong);
            assert!(b < STANDARD_BITS, "{kdf:?} reaches the universe only at {b} bits");
        }
    }

    #[test]
    fn the_bar_saturates_at_the_never_threshold() {
        assert_eq!(bar_fill(0.0), 0.0);
        assert_eq!(bar_fill(128.0), 1.0);
        assert_eq!(bar_fill(400.0), 1.0);
        assert!((bar_fill(64.0) - 0.5).abs() < 1e-6);
    }
}

#[cfg(test)]
mod render_preview {
    use super::*;

    #[test]
    fn entropy_rows_fit_the_frame() {
        const INNER: usize = 80;
        const GUTTER: usize = 17;
        const TRAIL: usize = 3;
        const CELLS: usize = 10;

        let samples = [
            "", "1", "123456", "hunter2", "Tr0ub4dor&3", "correct horse",
            "correct horse battery staple", "cQ7#vN2pLx9!mR4tZ8wK",
            "\u{2764}\u{2764}\u{2764}\u{2764}\u{2764}\u{2764}\u{2764}\u{2764}",
        ];

        println!("\n\u{250c}{}\u{2510}", "\u{2500}".repeat(INNER));
        for s in samples {
            let b = bits(s);
            let filled = ((bar_fill(b) * CELLS as f32).round() as usize).min(CELLS);
            let bar = format!("{}{}", "\u{2588}".repeat(filled), "\u{2591}".repeat(CELLS - filled));

            let top = format!(
                "{}{bar}  {} bits",
                format!("{:<GUTTER$}", "   entropy"),
                b.round() as u64,
            );
            let v = crack_verdict(b, Kdf::Argon2id);
            let cost = if v.numeric {
                format!("  \u{b7}  {}", crack_cost_phrase(b, Kdf::Argon2id))
            } else {
                String::new()
            };
            let bottom = format!("{}{}{cost}", format!("{:<GUTTER$}", "   time to crack"), v.phrase);

            for line in [&top, &bottom] {
                assert!(
                    line.chars().count() <= INNER - TRAIL,
                    "{:?} renders {} columns, frame allows {}",
                    s, line.chars().count(), INNER - TRAIL,
                );
                println!("\u{2502}{:<INNER$}\u{2502}", line);
            }
            println!("\u{2502}{:<INNER$}\u{2502}", format!("{:<GUTTER$}{s:?}", "   key"));
            println!("\u{251c}{}\u{2524}", "\u{2500}".repeat(INNER));
        }
        println!("\u{2514}{}\u{2518}", "\u{2500}".repeat(INNER));
    }
}

