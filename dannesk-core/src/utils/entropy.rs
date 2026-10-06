//! What a storage key is actually worth, in bits and in what breaking it costs.
//!
//! This replaced a pair of five-cell "security / convenience" meters on the
//! storage step. Those meters rated the *mode*, so they said the same thing
//! whatever the user typed — a six-character key and a thirty-character one lit
//! the same three cells. The numbers here move on every keystroke and are the
//! only thing on that screen that describes the user's own choice.
//!
//! ## The arithmetic is measured, not assumed
//!
//! [`ARGON2ID_CORE_SECONDS`] is this project's own KDF timed on the development
//! machine, and it is what makes a cost figure meaningful: an attacker who has
//! the file must run that same KDF once per guess. At the rented-compute price
//! below, a million guesses costs $3.34 — which is the number that settled the
//! "is a short key defensible on disk" question, and the number a user weighing
//! a quick key against a long one deserves to see.
//!
//! ## The honest caveat, and why the UI no longer states it
//!
//! [`bits`] assumes every character was chosen at random. For a *generated* key
//! that is true. For one a person invented it is an **upper bound**, and often a
//! wild one: `Password123!` scores 78 bits here and falls to a wordlist in
//! seconds. This module does not try to detect that — password-strength
//! estimators score the population rather than the person, and an attacker who
//! knows something about *this* user does better than any of them.
//!
//! The rows used to carry `· if randomly chosen` as a disclaimer. It came out
//! 2026-08-21: it qualified a number nobody read it against, and it spent the
//! one slot on the row that could instead say something the reader can act on.
//! [`stretch_phrase`] took the slot — the KDF cost is *why* the figures beside
//! it are what they are, and stating it turns two abstract numbers into one
//! mechanism. The caveat now lives in the colour: below [`bits`] of thirty the
//! row reads in the danger tone, which is the same warning delivered where it
//! cannot be skipped.

/// Core-seconds one Argon2id guess costs at our parameters (64 MB, t=3, p=4),
/// measured on the development machine. An attacker pays this per guess.
const ARGON2ID_CORE_SECONDS: f64 = 0.40085;

/// Core-seconds one guess at a **25th word** costs: BIP39's own stretch —
/// PBKDF2-HMAC-SHA512 × 2048 rounds, i.e. `Mnemonic::to_seed` — measured the
/// same way (release build, development machine). Nearly three hundred times
/// cheaper than a guess at the storage key, which is why there are two numbers
/// and not one: the 25th word is never written to disk and never passes under
/// our Argon2id. The attacker this models already holds the 24 words and is
/// guessing the one thing they don't have, and BIP39 is all that slows them.
const BIP39_CORE_SECONDS: f64 = 0.00144;

/// What stretches a secret — and so what one guess at it costs.
///
/// The two secrets a person picks on the setup screens are protected by
/// different things: the storage key by our KDF over the encrypted file, the
/// 25th word by BIP39's PBKDF2 alone. Quoting both against Argon2id, as the
/// rows once did, overstated a 25th word's `time to crack` by two orders of
/// magnitude.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kdf {
    /// The storage key: Argon2id, ours.
    Argon2id,
    /// The 25th word: PBKDF2, BIP39's, run by whoever holds the 24 words.
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

/// Rented compute, USD per core-hour — commodity cloud, spot-ish pricing.
const USD_PER_CORE_HOUR: f64 = 0.03;

/// The fleet the time figure is quoted against: a funded attacker renting ten
/// thousand cores (~$300/hour). Not a nation state, and not a lone laptop —
/// a number a motivated thief could actually put on a credit card.
pub const ATTACKER_CORES: f64 = 10_000.0;

/// Where the bar reads full. 2^128 is the conventional "no brute force will ever
/// finish" threshold, so the bar measures against forever rather than against
/// whatever we happen to require.
const FULL_SCALE_BITS: f64 = 128.0;

const SECONDS_PER_YEAR: f64 = 31_556_952.0;

/// Age of the universe, in years. Past this a duration stops being a quantity
/// and becomes a category, so the row says so rather than printing a number
/// nobody can hold — and a reader who cannot picture 67 billion years can
/// picture "longer than the universe" exactly.
const UNIVERSE_YEARS: f64 = 13.8e9;

/// World GDP, near enough. Serves the same purpose for money: above it, the
/// figure has left the space of things that can be spent.
const WORLD_GDP_USD: f64 = 1.1e14;

/// Shannon entropy of `secret` **on the assumption that its characters were
/// chosen at random**: `len × log2(pool)`, where the pool is the union of the
/// character classes present.
///
/// Using the classes *present* rather than all of ASCII is the standard model
/// and the conservative one here: adding a digit to a lowercase key raises the
/// pool from 26 to 36 for every position, which is what actually happens to an
/// attacker who must now cover both classes.
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
    if symbol { pool += 33 } // printable ASCII punctuation + space
    // Anything outside ASCII: credit a deliberately modest pool. The real space
    // is enormous, but a person reaching for non-ASCII picks from their own
    // keyboard, not from Unicode.
    if wide { pool += 100 }

    secret.chars().count() as f64 * (pool.max(2) as f64).log2()
}

/// Fraction of the bar to fill, clamped to `0.0..=1.0`.
pub fn bar_fill(bits: f64) -> f32 {
    (bits / FULL_SCALE_BITS).clamp(0.0, 1.0) as f32
}

/// Guesses an attacker expects to make: half the space.
fn expected_guesses(bits: f64) -> f64 {
    if bits <= 0.0 {
        return 0.0;
    }
    // 2^(bits-1), via exp2 so large exponents stay finite as f64 (up to ~1024).
    (bits - 1.0).exp2()
}

/// USD of rented compute to break a key of this size, on average. Independent of
/// how many cores are thrown at it — buying twice the cores halves the time and
/// costs the same.
pub fn crack_cost_usd(bits: f64, kdf: Kdf) -> f64 {
    expected_guesses(bits) * kdf.core_seconds() * (USD_PER_CORE_HOUR / 3600.0)
}

/// Seconds to break a key of this size on [`ATTACKER_CORES`] cores, on average.
pub fn crack_seconds(bits: f64, kdf: Kdf) -> f64 {
    expected_guesses(bits) * kdf.core_seconds() / ATTACKER_CORES
}

/// How the `time to crack` reads, and which colour it reads in.
///
/// One ladder produces both, which is the whole point: the phrase and the
/// colour used to come from different quantities — the words from time, the
/// colour from bits — and the two disagreed. "longer than the universe" landed
/// at ~74 bits for the storage key, in the plain band, and at ~82 bits for the
/// 25th word, in green. Same words, two colours. Deriving the colour from the
/// same rung as the words makes that impossible, and the sweep test below
/// proves it for every bit count and both KDFs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tier {
    /// Falls within a year. Danger colour.
    Weak,
    /// Outlasts a year, but not the universe. Amber.
    Fair,
    /// Longer than the universe has existed, and beyond. Green.
    Strong,
}

/// The `time to crack` read: its words, its rung, and whether the words are a
/// number (`3 years`) or a comparison (`longer than the age of the sun`). The
/// cost rides beside a number only — a metaphor with a dollar figure after it
/// is two claims fighting for one row, and the long rungs don't leave room.
pub struct Verdict {
    pub phrase: String,
    pub tier: Tier,
    pub numeric: bool,
}

/// The rungs, in years. Time-based up to the universe; the last three are
/// guess-space milestones and so KDF-independent (see [`ATOMS_BITS`]).
const HISTORY_YEARS: f64 = 5_000.0;
const HUMANKIND_YEARS: f64 = 300_000.0;
const SUN_YEARS: f64 = 4.6e9;

/// 2^128 is the conventional "no brute force will ever finish" line.
const STANDARD_BITS: f64 = 128.0;

/// Where the guess space starts to be spoken of in atoms. The observable
/// universe holds ~10^80 of them — 266 bits — with estimates running 10^78 to
/// 10^82 (259 to 272 bits). "Approaching" from 200; "more than" only from 300,
/// which clears every estimate, so the last rung cannot be argued with. A
/// 24-word mnemonic's own 256 bits (~10^77) sits, correctly, in "approaching".
const ATOMS_NEAR_BITS: f64 = 200.0;
const ATOMS_BITS: f64 = 300.0;

/// The longest phrase the ladder can produce, in characters. The rows that
/// draw it have a fixed budget (see `wallet_setup`), so every rung is pinned
/// under this by the test below rather than discovered on screen.
pub const PHRASE_MAX: usize = 40;

/// The `time to crack` read for `bits` under `kdf`.
///
/// Deliberately does **not** name the fleet it is quoted against. "18 days on
/// 10,000 cores" is precise and unreadable — a core is not a unit anyone outside
/// this trade thinks in. The number beside it carries the same claim in a unit
/// everyone owns: dollars. The two are one consistent story: both describe the
/// same attacker — [`ATTACKER_CORES`] rented at [`USD_PER_CORE_HOUR`] — so the
/// time is what they wait and the cost is what they pay.
///
/// Past the universe the ladder switches from time to the size of the space
/// itself, because 128 and 256 bits are milestones people know by number and
/// the KDF no longer changes anything worth saying.
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

/// The work factor the other figures rest on, right-pinned on the entropy row:
/// what one guess costs an attacker before any of the arithmetic above applies.
///
/// Derived from [`ARGON2ID_CORE_SECONDS`] rather than written out, because a
/// hand-typed "~0.8 s/guess" is exactly the copy that survives the next
/// re-measurement of the KDF and starts lying about it. One decimal: the
/// constant is calibrated on one machine and the third digit is noise.
pub fn stretch_phrase() -> String {
    format!("~{:.1} s/guess", ARGON2ID_CORE_SECONDS)
}

/// The `time to crack` row's right half: what that compute costs to rent.
pub fn crack_cost_phrase(bits: f64, kdf: Kdf) -> String {
    let usd = crack_cost_usd(bits, kdf);
    if usd > WORLD_GDP_USD {
        return "beyond world GDP".to_string();
    }
    humanise_cost(usd)
}

/// A duration as a person would say it. Rounded down to whole units, because a
/// figure quoted longer than it is would be the one lie this screen cannot tell.
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

/// USD as a person would say it.
pub fn humanise_cost(usd: f64) -> String {
    match usd {
        c if c < 0.01 => "under a cent".to_string(),
        c if c < 1.0 => format!("{:.0} cents", c * 100.0),
        c if c < 1_000.0 => format!("${c:.0}"),
        c => format!("${}", magnitude(c)),
    }
}

/// Big numbers in words: `4.2 billion`, `310 trillion`. Beyond a quintillion the
/// scale stops meaning anything to anyone, so it says so instead of printing
/// digits nobody can read.
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
        // Thousands read better with a separator than as "1.2 thousand".
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

    /// The pool is the union of the classes present, so each new class the user
    /// reaches for is worth real bits per character.
    #[test]
    fn the_character_pool_grows_with_the_classes_used() {
        assert_eq!(bits(""), 0.0);
        // Six digits: pool 10, so 6·log2(10) ≈ 19.9 — the ~20 bits that makes a
        // numeric PIN indefensible against an offline copy of the file.
        assert!((bits("123456") - 19.93).abs() < 0.01);
        // Same length, four classes: pool 95.
        assert!(bits("aB3$xY") > bits("123456"));
        // Length is the cheap axis, and it should dominate.
        assert!(bits("aaaaaaaaaaaaaaaaaaaa") > bits("aB3$xY"));
    }

    /// The cost model has to reproduce the figure it was derived from: a million
    /// Argon2id guesses is $3.34 of rented compute. `expected_guesses` is half
    /// the space, so a space of 2·10^6 is what costs that.
    #[test]
    fn a_million_guesses_costs_three_dollars_and_change() {
        let bits_for_2m = (2.0e6_f64).log2();
        let cost = crack_cost_usd(bits_for_2m, Kdf::Argon2id);
        assert!((cost - 3.34).abs() < 0.01, "million guesses cost ${cost:.2}, expected $3.34");
    }

    /// Six digits must come out cheap enough to be alarming — this is the whole
    /// reason the screen shows a cost at all.
    #[test]
    fn six_digits_is_pocket_change() {
        let cost = crack_cost_usd(bits("123456"), Kdf::Argon2id);
        assert!(cost < 5.0, "six digits cost ${cost:.2}");
        assert!(crack_seconds(bits("123456"), Kdf::Argon2id) < 60.0, "six digits should fall in under a minute");
    }

    /// A long generated key must land beyond any budget, or the display would be
    /// telling people their good keys are weak.
    #[test]
    fn a_long_random_key_is_out_of_reach() {
        let strong = bits("cQ7#vN2pLx9!mR4tZ8wK");
        assert!(strong > 100.0, "20 mixed chars scored {strong} bits");
        assert!(crack_cost_usd(strong, Kdf::Argon2id) > 1e15);
    }

    /// Every branch has to produce something sayable, and nothing may render as
    /// `inf`, `NaN` or a wall of digits.
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

    /// The same word is far cheaper to guess as a 25th word than as a storage
    /// key: there is no Argon2id between the attacker and it, only BIP39's own
    /// PBKDF2. The screen has to say so, or a short 25th word looks safe.
    #[test]
    fn a_25th_word_is_cheaper_to_guess_than_a_key() {
        let b = bits("hello");
        let ratio = crack_cost_usd(b, Kdf::Argon2id) / crack_cost_usd(b, Kdf::Bip39);
        assert!(ratio > 100.0, "25th word only {ratio:.0}x cheaper than the key");
        assert!(crack_seconds(b, Kdf::Bip39) < 1.0, "\"hello\" as a 25th word should fall instantly");
    }

    /// The one rule the ladder exists for: no phrase is ever drawn in two
    /// colours. Swept at every tenth of a bit for both KDFs, because the rungs
    /// are placed in time and the KDF moves where in bits each one lands.
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
                // Numeric rungs vary with the KDF; the words must not.
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
        // Every rung was actually reached by the sweep.
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

    /// The colours mean what they say: a year's grace is where danger ends,
    /// the universe's age is where reassurance begins — under either KDF.
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

    /// The bar is a fraction of "never", so it saturates rather than overflowing.
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

    /// Renders the two rows exactly as the storage step draws them, at the
    /// widths that step uses, and proves they clear the frame at every size a
    /// key can reach — including the extremes, where the phrases are longest.
    /// `cargo test -- --nocapture entropy_rows` prints the box.
    ///
    /// The geometry is restated here rather than imported: this module is
    /// arithmetic and owes the UI nothing, and the point of the preview is to
    /// fail loudly if the two ever disagree. Keep it in step with
    /// `ui::components::wallet_setup`'s `INNER` / `GUTTER` / `TRAIL`.
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

            // The work factor is pinned to the right margin, so the row is
            // measured as left + at least two columns of gap + right.
            let left = format!(
                "{}{bar}  {} bits",
                format!("{:<GUTTER$}", "   entropy"),
                b.round() as u64,
            );
            let right = format!("stretch  argon2id \u{b7} {}", stretch_phrase());
            let top = format!("{left}  {right}");
            // Mirrors `wallet_setup::crack_runs`: the cost rides beside a
            // number, never beside a comparison.
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

