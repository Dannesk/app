use std::ops::Range;

use dannesk_btc_codec::bip39::WORDS;
use zeroize::Zeroize;

use crate::secure::SecureString;

pub const SLOTS: usize = 24;
pub const WORD_MAX: usize = 8;
pub const NEAR_MAX: usize = 3;
pub const MASK: &str = "\u{b7}\u{b7}\u{b7}\u{b7}\u{b7}";

pub fn list() -> &'static [&'static str] {
    &WORDS
}

pub fn canonical(w: &str) -> Option<&'static str> {
    list().binary_search(&w).ok().map(|i| list()[i])
}

pub fn fields(text: &str) -> Vec<Range<usize>> {
    let mut out = Vec::with_capacity(SLOTS);
    let mut start = 0;
    let mut n = 0;
    for c in text.chars() {
        if c == ' ' {
            out.push(start..n);
            start = n + 1;
        }
        n += 1;
    }
    out.push(start..n);
    out
}

pub fn word<'t>(text: &'t str, chars: &Range<usize>) -> &'t str {
    let mut at = text.char_indices().map(|(b, _)| b).chain(std::iter::once(text.len()));
    let start = at.nth(chars.start).unwrap_or(text.len());
    let end = if chars.end > chars.start {
        at.nth(chars.end - chars.start - 1).unwrap_or(text.len())
    } else {
        start
    };
    &text[start..end]
}

pub fn count(text: &str) -> usize {
    text.split_whitespace().count()
}

pub fn matches(prefix: &str) -> &'static [&'static str] {
    if prefix.is_empty() {
        return &[];
    }
    let all = list();
    let lo = all.partition_point(|w| *w < prefix);
    let n = all[lo..].partition_point(|w| w.starts_with(prefix));
    &all[lo..lo + n]
}

pub fn is_word(w: &str) -> bool {
    list().binary_search(&w).is_ok()
}

pub fn nearest(w: &str) -> Vec<&'static str> {
    if w.is_empty() || w.len() > WORD_MAX + 2 {
        return Vec::new();
    }
    let (mut one, mut two) = (Vec::new(), Vec::new());
    for &x in list() {
        match osa(w.as_bytes(), x.as_bytes()) {
            1 => one.push(x),
            2 => two.push(x),
            _ => {}
        }
    }
    let mut out = if one.is_empty() { two } else { one };
    out.truncate(NEAR_MAX);
    out
}

fn osa(a: &[u8], b: &[u8]) -> usize {
    const N: usize = WORD_MAX + 3;
    if a.len().abs_diff(b.len()) > 2 || a.len() >= N || b.len() >= N {
        return usize::MAX;
    }
    let mut d = [[0usize; N]; N];
    for (i, row) in d.iter_mut().enumerate().take(a.len() + 1) {
        row[0] = i;
    }
    for j in 0..=b.len() {
        d[0][j] = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            let mut best = (d[i - 1][j] + 1).min(d[i][j - 1] + 1).min(d[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                best = best.min(d[i - 2][j - 2] + 1);
            }
            d[i][j] = best;
        }
    }
    d[a.len()][b.len()]
}

pub fn bad_slots(text: &str) -> Vec<usize> {
    fields(text)
        .iter()
        .enumerate()
        .filter(|(_, r)| {
            let w = word(text, r);
            !w.is_empty() && !is_word(w)
        })
        .map(|(k, _)| k)
        .collect()
}

fn set_word(buf: &mut SecureString, k: usize, new: &str) {
    let Some(r) = fields(buf.as_str()).get(k).cloned() else { return };
    for _ in r.clone() {
        buf.remove(r.start);
    }
    buf.insert_str(r.start, new);
}

fn ensure_fields(buf: &mut SecureString, n: usize) {
    while fields(buf.as_str()).len() < n.min(SLOTS) {
        let end = buf.char_len();
        buf.insert(end, ' ');
    }
}

fn open_slot(buf: &mut SecureString) {
    let f = fields(buf.as_str());
    let filled = f.iter().filter(|r| !r.is_empty()).count();
    if filled >= SLOTS || f.len() >= SLOTS || f.iter().any(|r| r.is_empty()) {
        return;
    }
    let end = buf.char_len();
    buf.insert(end, ' ');
}

pub fn type_char(buf: &mut SecureString, k: usize, at: usize, c: char) {
    if !c.is_ascii_lowercase() {
        return;
    }
    let Some(r) = fields(buf.as_str()).get(k).cloned() else { return };
    if r.len() < WORD_MAX {
        buf.insert(r.start + at.min(r.len()), c);
    }
}

pub fn erase(buf: &mut SecureString, k: usize, at: usize) {
    let Some(r) = fields(buf.as_str()).get(k).cloned() else { return };
    if at < r.len() {
        buf.remove(r.start + at);
    }
}

pub fn step_back(buf: &mut SecureString, k: usize) {
    let f = fields(buf.as_str());
    if k > 0 && k + 1 == f.len() && f[k].is_empty() {
        buf.remove(f[k].start - 1);
    }
}

pub fn take(buf: &mut SecureString, k: usize, lit: usize, tab: bool) {
    let choice = {
        let text = buf.as_str();
        fields(text).get(k).and_then(|r| {
            let m = matches(word(text, r));
            m.get(lit.min(m.len().saturating_sub(1))).copied()
        })
    };
    match choice {
        Some(w) => {
            set_word(buf, k, w);
            open_slot(buf);
        }
        None if tab => open_slot(buf),
        None => {}
    }
}

pub fn pick(buf: &mut SecureString, k: usize, n: usize) {
    let choice = {
        let text = buf.as_str();
        fields(text).get(k).and_then(|r| {
            let w = word(text, r);
            let m = matches(w);
            if !m.is_empty() {
                m.get(n).copied()
            } else {
                nearest(w).get(n).copied()
            }
        })
    };
    if let Some(w) = choice {
        set_word(buf, k, w);
        open_slot(buf);
    }
}

pub fn advance(buf: &mut SecureString) {
    open_slot(buf);
}

pub fn replace(buf: &mut SecureString, k: usize, c: Option<char>) {
    match c {
        Some(c) if c.is_ascii_lowercase() => {
            let mut tmp = [0u8; 4];
            set_word(buf, k, c.encode_utf8(&mut tmp));
            tmp.zeroize();
        }
        Some(_) => {}
        None => set_word(buf, k, ""),
    }
}

pub fn fill(buf: &mut SecureString, k: usize, text: &str) -> (usize, usize) {
    let mut lower = text.to_ascii_lowercase();
    let (put, total) = {
        let tokens: Vec<&str> = lower
            .split(|c: char| !c.is_ascii_lowercase())
            .filter(|t| !t.is_empty())
            .collect();
        let total = tokens.len();
        let start = if total >= SLOTS { 0 } else { k.min(SLOTS - 1) };
        let mut put = 0;
        for (j, t) in tokens.iter().enumerate() {
            let slot = start + j;
            if slot >= SLOTS {
                break;
            }
            ensure_fields(buf, slot + 1);
            set_word(buf, slot, t);
            put += 1;
        }
        (put, total)
    };
    lower.zeroize();
    if total > 0 {
        open_slot(buf);
    }
    (put, total)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buf(s: &str) -> SecureString {
        SecureString::new(s.to_string())
    }

    #[test]
    fn fields_keep_empty_slots_in_place() {
        assert_eq!(fields(""), vec![0..0]);
        assert_eq!(fields("abandon"), vec![0..7]);
        assert_eq!(fields("abandon "), vec![0..7, 8..8]);
        assert_eq!(fields("a  c"), vec![0..1, 2..2, 3..4]);
        assert_eq!(word("abandon ability", &(8..15)), "ability");
        assert_eq!(count("a  c "), 2);
    }

    #[test]
    fn matching_follows_the_sorted_list() {
        assert_eq!(matches("dri"), &["drift", "drill", "drink", "drip", "drive"]);
        assert_eq!(matches("can")[0], "can");
        assert_eq!(matches("harb"), &["harbor"]);
        assert!(matches("drq").is_empty());
        assert!(matches("").is_empty());
        assert!(matches("zoo").contains(&"zoo"));
    }

    #[test]
    fn every_word_resolves_from_four_letters() {
        for w in list() {
            let stub = &w[..w.len().min(4)];
            assert_eq!(matches(stub)[0], *w, "{stub} does not lead to {w}");
        }
    }

    #[test]
    fn nearest_counts_a_swap_as_one_slip() {
        assert!(nearest("harbour").contains(&"harbor"));
        assert!(nearest("ivroy").contains(&"ivory"));
        assert!(nearest("drq").len() <= NEAR_MAX);
        assert!(nearest("").is_empty());
    }

    #[test]
    fn taking_a_match_fills_the_slot_and_opens_the_next() {
        let mut b = buf("harb");
        take(&mut b, 0, 0, false);
        assert_eq!(b.as_str(), "harbor ");
        type_char(&mut b, 1, 0, 'c');
        type_char(&mut b, 1, 1, 'a');
        type_char(&mut b, 1, 2, 'n');
        take(&mut b, 1, 0, false);
        assert_eq!(b.as_str(), "harbor can ");
        let mut b = buf("dri");
        pick(&mut b, 0, 1);
        assert_eq!(b.as_str(), "drill ");
    }

    #[test]
    fn no_match_moves_on_only_by_tab() {
        let mut b = buf("drq");
        take(&mut b, 0, 0, false);
        assert_eq!(b.as_str(), "drq");
        take(&mut b, 0, 0, true);
        assert_eq!(b.as_str(), "drq ");
        let mut b = buf("harbour");
        pick(&mut b, 0, 0);
        assert!(is_word(fields(b.as_str()).first().map(|r| word(b.as_str(), r)).unwrap()));
    }

    #[test]
    fn a_word_never_outgrows_the_longest_on_the_list() {
        let mut b = buf("abstract");
        type_char(&mut b, 0, 8, 'x');
        assert_eq!(b.as_str(), "abstract");
        type_char(&mut b, 0, 0, 'Q');
        assert_eq!(b.as_str(), "abstract");
    }

    #[test]
    fn emptying_a_middle_word_keeps_the_others_in_their_slots() {
        let mut b = buf("canyon gravity trophy");
        replace(&mut b, 1, None);
        assert_eq!(b.as_str(), "canyon  trophy");
        assert_eq!(fields(b.as_str()).len(), 3);
        replace(&mut b, 1, Some('g'));
        assert_eq!(b.as_str(), "canyon g trophy");
    }

    #[test]
    fn stepping_back_removes_only_a_trailing_open_slot() {
        let mut b = buf("canyon ");
        step_back(&mut b, 1);
        assert_eq!(b.as_str(), "canyon");
        let mut b = buf("canyon  trophy");
        step_back(&mut b, 1);
        assert_eq!(b.as_str(), "canyon  trophy");
    }

    #[test]
    fn a_numbered_list_pastes_from_the_first_slot() {
        let words: Vec<&str> = list().iter().take(24).copied().collect();
        let numbered: String = words.iter().enumerate().map(|(i, w)| format!("{}. {}\n", i + 1, w.to_uppercase())).collect();
        let mut b = buf("");
        let (put, total) = fill(&mut b, 5, &numbered);
        assert_eq!((put, total), (24, 24));
        assert_eq!(b.as_str(), words.join(" "));
    }

    #[test]
    fn a_short_paste_fills_from_the_caret_and_a_long_one_stops_at_24() {
        let mut b = buf("canyon gravity ");
        let (put, total) = fill(&mut b, 2, "trophy shallow");
        assert_eq!((put, total), (2, 2));
        assert_eq!(b.as_str(), "canyon gravity trophy shallow ");
        let long = vec!["abandon"; 25].join(" ");
        let mut b = buf("");
        let (put, total) = fill(&mut b, 0, &long);
        assert_eq!((put, total), (24, 25));
        assert_eq!(count(b.as_str()), 24);
        assert_eq!(fields(b.as_str()).len(), 24);
    }

    #[test]
    fn a_full_phrase_never_grows_an_open_slot() {
        let full = vec!["abandon"; 24].join(" ");
        let mut b = buf(&full);
        advance(&mut b);
        assert_eq!(b.as_str(), full);
    }

    #[test]
    fn bad_slots_name_the_words_off_the_list() {
        assert_eq!(bad_slots("canyon harbour  ivroy "), vec![1, 3]);
        assert!(bad_slots("").is_empty());
    }
}
