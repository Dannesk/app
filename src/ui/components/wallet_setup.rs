

use iced::widget::{Column, Row};
use iced::Widget as _;
use iced::widget::{button, column, container, row, stack, text, Space};
use iced::{Alignment, Border, Color, Element, Length, Padding, Shadow};
use crate::controller::app_state::{AppState, ImportMode};
use crate::controller::message::{Message, SecureField};
use crate::secure::SecureString;
use crate::ui::components::compact::{self, minimum_hint};
use crate::ui::components::tui::{self, Run};
use crate::ui::components::wallet_gate::{self, Mark};
use crate::utils::fonts::MONO;
use crate::utils::entropy;
use crate::utils::theme;


pub fn mnemonic_checksum_ok(phrase: &str) -> bool {
    dannesk_btc_codec::bip39::Mnemonic::parse(phrase).is_ok()
}

pub(crate) const TARGET_WORDS: usize = 24;


pub(crate) fn held_bits(secret: &SecureString, hold: Option<f64>) -> f64 {
    if secret.char_len() == 0 {
        hold.unwrap_or(0.0)
    } else {
        entropy::bits(secret.as_str().trim())
    }
}


const CHECKSUM_PENDING: &str = "awaiting all 24 words";
const CHECKSUM_VALID: &str = "valid checksum";
const CHECKSUM_INVALID: &str = "invalid checksum";

pub(crate) const PHRASE_PLACEHOLDER: &str = "paste or type your 24 words…";

const PHRASE_KEYS: &[(&str, &str)] = &[("ctrl+v", "paste"), ("enter", "next word")];

const CREATE_KEYS: &[(&str, &str)] = &[("ctrl+v", "paste")];


pub const KEY_MIN: usize = 6;


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetupFlow {
    Import,
    Create,
}


pub const WORD25_MIN: usize = 6;


pub fn can_continue_word25(has_word: bool, bip39: &SecureString) -> bool {
    !has_word || bip39.trimmed_char_len() >= WORD25_MIN
}


fn word25_placeholder(flow: SetupFlow) -> &'static str {
    match flow {
        SetupFlow::Import => super::grid::BIP39_PLACEHOLDER,
        SetupFlow::Create => "optional 25th word - 6 minimum characters",
    }
}


fn can_submit(mode: ImportMode, secret: &SecureString) -> bool {
    match mode {
        ImportMode::Standard => secret.trimmed_char_len() >= KEY_MIN,
        ImportMode::Cold => true,
    }
}

const FRAME_W: f32 = 800.0;
const LEFT_W: f32 = 403.0;
const RIGHT_W: f32 = 396.0;
const PANE_PAD_V: f32 = 12.0;
const PANE_PAD_H: f32 = 16.0;


const PANES_H: f32 = 386.0;


const PHRASE_BOX_H: f32 = 154.0;


const GRID_PLACEHOLDER: &str = "\u{b7}\u{b7}\u{b7}\u{b7}\u{b7}";


const NEVER_SENT: &str = "generated on this device \u{2014} never sent anywhere";
const NOT_YET: &str = "nothing generated yet";


const TYPE_CARD_H: f32 = 52.0;
const TYPE_CARD_PREFIX: f32 = 14.0;
const TYPE_CARD_NAME: f32 = 9.0;


const PANE_FIG: f32 = 140.0;


const COLD_NOTE: &str = "Your seed is never written to disk. You type your 24 words \
each time you sign.";

const COLD_SIGN: &str = "24 word mnemonic";


pub fn can_finish(
    flow:   SetupFlow,
    seed:   &SecureString,
    word25: bool,
    w25:    &SecureString,
    mode:   ImportMode,
    key:    &SecureString,
) -> bool {
    let words = seed.as_str().split_whitespace().count();
    let phrase_ok = match flow {
        SetupFlow::Create => words == TARGET_WORDS,
        _ => words == TARGET_WORDS && mnemonic_checksum_ok(seed.as_str()),
    };
    phrase_ok && can_continue_word25(word25, w25) && can_submit(mode, key)
}

pub struct SetupScreenParams<'a> {
  
    pub flow: SetupFlow,
    pub title: &'static str,
 
    pub path: &'static str,
  
    pub mark: Mark,

    pub address_types: Vec<(&'static str, &'static str, bool, Message)>,
  
    pub address_type_bip: &'static str,
    pub seed_field: SecureField,
    pub seed: &'a SecureString,
    pub seed_reveal: bool,
    pub copied: bool,
    pub on_generate: Option<Message>,
    pub on_copy: Option<Message>,
    pub word25: bool,
    pub w25_field: SecureField,
    pub w25: &'a SecureString,
    pub w25_reveal: bool,
    pub on_word25_no: Message,
    pub on_word25_yes: Message,
    pub mode: ImportMode,
    pub enc_field: SecureField,
    pub enc: &'a SecureString,
    pub enc_reveal: bool,
    pub on_mode_device: Message,
    pub on_mode_cold: Message,
    pub on_back: Message,
    pub on_submit: Message,
    pub submit_label: &'static str,
}

pub fn setup_screen<'a>(state: &'a AppState, prm: SetupScreenParams<'a>) -> Element<'a, Message> {
    let cp = theme::compact(&state.theme);
    let scale = state.scale();

    let left: Element<'a, Message> = match prm.flow {
        SetupFlow::Create => create_pane(&prm, cp, scale),
        _ => import_pane(&prm, cp, scale),
    };
    let right = keys_pane(&prm, state, cp, scale);

    let pane_pad = Padding::new(0.0)
        .top(PANE_PAD_V * scale)
        .bottom(PANE_PAD_V * scale)
        .left(PANE_PAD_H * scale)
        .right(PANE_PAD_H * scale);

    let panes = row![
        container(left)
            .width(Length::Fixed(LEFT_W * scale))
            .height(Length::Fixed(PANES_H * scale))
            .padding(pane_pad),
        compact::vrule(cp.rule, PANES_H * scale),
        container(right)
            .width(Length::Fixed(RIGHT_W * scale))
            .height(Length::Fixed(PANES_H * scale))
            .padding(pane_pad),
    ];

    let shortcuts = match prm.flow {
        SetupFlow::Create => CREATE_KEYS,
        _ => PHRASE_KEYS,
    };
    let frame = column![
        compact::header_strip(prm.title, shortcuts, cp, scale),
        panes,
    ]
    .width(Length::Fixed(FRAME_W * scale));

  
    stack![
        container(frame)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center),
        container(compact::back_chevron(prm.on_back.clone(), cp, scale))
            .padding(Padding::new(0.0).top(compact::CORNER_TOP * scale).left(compact::CORNER_LEFT * scale)),
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .boxed()
}


fn compact_checksum_verdict(
    complete: bool,
    ok:       bool,
    cp:       &'static theme::CompactPalette,
) -> (&'static str, Color) {
    match (complete, ok) {
        (false, _) => (CHECKSUM_PENDING, cp.muted),
        (true, true) => (CHECKSUM_VALID, cp.green),
        (true, false) => (CHECKSUM_INVALID, cp.red),
    }
}

fn import_pane<'a>(
    prm:   &SetupScreenParams<'a>,
    cp:    &'static theme::CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    let words = prm.seed.as_str().split_whitespace().count();
    let complete = words == TARGET_WORDS;
    let checksum_ok = complete && mnemonic_checksum_ok(prm.seed.as_str());


    let counter_colour = match words {
        0 => cp.faint,
        n if n == TARGET_WORDS => {
            if checksum_ok { cp.green } else { cp.red }
        }
        _ => cp.amber,
    };

    let verdict: Element<'a, Message> = if words == 0 {
        Space::new().boxed()
    } else {
        let (body, colour) = compact_checksum_verdict(complete, checksum_ok, cp);
        let glyph = match (complete, checksum_ok) {
            (true, true) => "\u{2713} ",
            (true, false) => "\u{2715} ",
            _ => "",
        };
        text(format!("{glyph}{body}")).size(compact::ROW * scale).color(colour).boxed()
    };

 
    let action = if words == 0 {
        compact::upper_link("Paste all", Message::SecurePasteRequested(prm.seed_field), cp, scale)
    } else {
        compact::upper_link("Clear", Message::SecureClearRequested(prm.seed_field), cp, scale)
    };

    let status = row![
        compact::mono_runs(
            vec![(format!("{words} / {TARGET_WORDS}"), counter_colour)],
            compact::ROW * scale,
        ),
        text(" words").size(compact::ROW * scale).color(cp.dim),
        Space::new().width(9.0 * scale),
        verdict,
        Space::new().width(Length::Fill),
        action,
    ]
    .align_y(Alignment::Center);

    column![
        compact::eyebrow_data("recovery phrase", &format!("{} \u{b7} {TARGET_WORDS}", prm.path), cp, scale),
        compact::phrase_box(
            prm.seed_field,
            prm.seed,
            prm.seed_reveal,
            PHRASE_PLACEHOLDER,
            PHRASE_BOX_H,
            prm.on_submit.clone(),
            cp,
            scale,
        ),
        Space::new().height(9.0 * scale),
        status,
        address_type_picker(prm, cp, scale),
    ]

    .height(Length::Fill)
    .boxed()
}


fn address_type_picker<'a>(
    prm:   &SetupScreenParams<'a>,
    cp:    &'static theme::CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    if prm.address_types.is_empty() {
        return container(wallet_gate::figure(prm.mark, cp, scale * PANE_FIG / wallet_gate::FIG))
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .boxed();
    }
    let mut cards: Row<Element<'_, Message>> = row![].spacing(6.0 * scale).width(Length::Fill);
    for (prefix, name, active, msg) in prm.address_types.iter().cloned() {
        cards = cards.push(type_card(prefix, name, active, msg, cp, scale));
    }
    container(
        column![
            compact::eyebrow("address type", prm.address_type_bip, cp, scale),
            cards,
        ]
        .width(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .center_y(Length::Fill)
    .boxed()
}


fn type_card<'a>(
    prefix: &'static str,
    name:   &'static str,
    active: bool,
    msg:    Message,
    cp:     &'static theme::CompactPalette,
    scale:  f32,
) -> Element<'a, Message> {
    let ink = if active { cp.text } else { cp.muted };
    button(
        column![
            text(prefix).font(MONO).size(TYPE_CARD_PREFIX * scale).color(ink),
            Space::new().height(3.0 * scale),
            text(name).size(TYPE_CARD_NAME * scale).color(if active { cp.dim } else { cp.faint }),
        ]
        .align_x(Alignment::Center)
        .width(Length::Fill),
    )
    .on_press(msg)
    .width(Length::Fill)
    .height(Length::Fixed(TYPE_CARD_H * scale))
    .padding(Padding::new(0.0).top(9.0 * scale).bottom(9.0 * scale))
    .style(move |_, status| {
        let hot = matches!(status, button::Status::Hovered | button::Status::Pressed);
        button::Style {
            background: Some(if active { cp.neutral } else { Color::TRANSPARENT }.into()),
            border: Border {
                color: if hot && !active { cp.focus } else { cp.border },
                width: 1.0,
                radius: (6.0 * scale).into(),
            },
            text_color: ink,
            shadow: Shadow::default(),
            snap: false,
        }
    })
    .boxed()
}


fn create_pane<'a>(
    prm:   &SetupScreenParams<'a>,
    cp:    &'static theme::CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    let words: Vec<&str> = prm.seed.as_str().split_whitespace().collect();
    let generated = words.len() == TARGET_WORDS;

    let mut grid: Column<Element<'_, Message>> = column![].spacing(3.5 * scale);
    for r in 0..compact::WORDS_ROWS {
        let mut runs: Vec<Run> = Vec::new();
        for c in 0..compact::WORDS_COLS {
            if c > 0 {
                runs.push((" ".repeat(compact::GAP_CHARS), cp.text));
            }
            let i = c * compact::WORDS_ROWS + r;
            runs.push((format!("{:02} ", i + 1), cp.faint));
            match words.get(i) {
                Some(w) => runs.push((tui::pad(w, compact::WORD_CHARS), cp.text)),
                None => runs.push((tui::pad(GRID_PLACEHOLDER, compact::WORD_CHARS), cp.faint)),
            }
        }
        grid = grid.push(compact::mono_runs(runs, compact::ROW * scale));
    }

    let border = if generated { cp.focus } else { cp.border };
    let boxed = container(grid)
        .width(Length::Fill)
        .height(Length::Fixed(PHRASE_BOX_H * scale))
        .padding(
            Padding::new(0.0)
                .top(9.0 * scale)
                .bottom(9.0 * scale)
                .left(11.0 * scale)
                .right(11.0 * scale),
        )
        .style(move |_| container::Style {
            background: Some(cp.field.into()),
            border: Border { color: border, width: 1.0, radius: (6.0 * scale).into() },
            ..Default::default()
        });

    let provenance: Element<'a, Message> = if generated {
        text(format!("\u{2713} {NEVER_SENT}"))
            .size(compact::ROW * scale)
            .color(cp.green)
            .boxed()
    } else {
        text(NOT_YET).size(compact::ROW * scale).color(cp.muted).boxed()
    };

    let action: Element<'a, Message> = if !generated {
        compact::upper_link(
            "Generate",
            prm.on_generate.clone().expect("create pane without on_generate"),
            cp,
            scale,
        )
    } else if prm.copied {
        text("\u{2713} copied").size(compact::ROW * scale).color(cp.green).boxed()
    } else {
        compact::upper_link(
            "Copy",
            prm.on_copy.clone().expect("create pane without on_copy"),
            cp,
            scale,
        )
    };

    let status = row![provenance, Space::new().width(Length::Fill), action]
        .align_y(Alignment::Center);

    column![
        compact::eyebrow_data("recovery phrase", &format!("{} \u{b7} {TARGET_WORDS}", prm.path), cp, scale),
        boxed,
        Space::new().height(9.0 * scale),
        status,
        address_type_picker(prm, cp, scale),
    ]
    .height(Length::Fill)
    .boxed()
}


fn keys_pane<'a>(
    prm:   &SetupScreenParams<'a>,
    state: &'a AppState,
    cp:    &'static theme::CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    let field_w = RIGHT_W - 2.0 * PANE_PAD_H;

    let mut col = column![
        compact::eyebrow("25th word", "optional", cp, scale),
        compact::segmented(
            ("No", !prm.word25, prm.on_word25_no.clone()),
            ("Yes", prm.word25, prm.on_word25_yes.clone()),
            cp,
            scale,
        ),
    ];

    if prm.word25 {
        col = col
            .push(Space::new().height(8.0 * scale).boxed())
            .push(compact::boxed_secret(
                prm.w25_field,
                prm.w25,
                prm.w25_reveal,
                word25_placeholder(prm.flow),
                field_w,
                Some(prm.on_submit.clone()),
                cp,
                scale,
            ))
            .push(Space::new().height(8.0 * scale).boxed())
     
            .push(compact::meter_rows(
                entropy::bits(prm.w25.as_str().trim()),
                entropy::Kdf::Bip39,
                cp,
                scale,
            ));
    }

    col = col
        .push(Space::new().height(12.0 * scale).boxed())
        .push(compact::hairline(cp.rule))
        .push(Space::new().height(11.0 * scale).boxed())
        .push(compact::eyebrow("key storage", "required", cp, scale))
        .push(compact::segmented(
            ("On this device", prm.mode == ImportMode::Standard, prm.on_mode_device.clone()),
            ("Cold storage", prm.mode == ImportMode::Cold, prm.on_mode_cold.clone()),
            cp,
            scale,
        ));

    match prm.mode {
        ImportMode::Standard => {
            let hint = minimum_hint(KEY_MIN, "characters");
            col = col
                .push(Space::new().height(8.0 * scale).boxed())
                .push(compact::boxed_secret(
                    prm.enc_field,
                    prm.enc,
                    prm.enc_reveal,
                    &hint,
                    field_w,
                    Some(prm.on_submit.clone()),
                    cp,
                    scale,
                ))
                .push(Space::new().height(8.0 * scale).boxed())
                .push(compact::meter_rows(
                    held_bits(prm.enc, state.entropy_hold),
                    entropy::Kdf::Argon2id,
                    cp,
                    scale,
                ))
      
                .push(compact::stat_rows(
                    &[("Cipher", "AES-256-GCM"), ("KDF", "Argon2id")],
                    cp,
                    scale,
                ));
        }
        ImportMode::Cold => {
            col = col
                .push(Space::new().height(8.0 * scale).boxed())
                .push(
                    text(COLD_NOTE)
                        .size(10.5 * scale)
                        .line_height(iced::widget::text::LineHeight::Relative(1.5))
                        .color(cp.dim).boxed(),
                )
                .push(compact::stat_rows(
                    &[("On disk", "nothing"), ("Sign with", COLD_SIGN)],
                    cp,
                    scale,
                ));
        }
    }

    let enabled = can_finish(prm.flow, prm.seed, prm.word25, prm.w25, prm.mode, prm.enc);
    col = col
        .push(Space::new().height(13.0 * scale).boxed())
        .push(compact::cta(prm.submit_label, enabled, prm.on_submit.clone(), cp, scale));

    col.boxed()
}

#[cfg(test)]
mod tests {
    use super::*;


    #[test]
    fn a_held_reading_survives_submit_but_not_a_retry() {
        let typed = SecureString::new("correct horse battery staple".to_string());
        let empty = SecureString::new(String::new());
        let live = entropy::bits(typed.as_str());

        assert_eq!(held_bits(&typed, None), live);
        assert_eq!(held_bits(&empty, None), 0.0);
        assert_eq!(held_bits(&empty, Some(live)), live);
        assert_eq!(held_bits(&typed, Some(1.0)), live);
    }

  
    #[test]
    fn the_cell_width_is_the_longest_bip39_word() {
        let longest = dannesk_btc_codec::bip39::WORDS
            .iter()
            .map(|w| w.chars().count())
            .max()
            .unwrap();
        assert_eq!(compact::WORD_CHARS, longest);
    }

    #[test]
    fn the_compact_pane_holds_the_word_grid() {
        let advance = compact::ROW * tui::MONO_ADVANCE;
        let line = compact::ROW * 1.6;

     
        let box_inner = LEFT_W - 2.0 * PANE_PAD_H - 11.0 - (11.0 + 13.0 + 6.0);
        assert!(
            compact::GRID_SPAN as f32 * advance <= box_inner,
            "the word grid ({} columns) is wider than the seed box",
            compact::GRID_SPAN,
        );

        assert!(
            compact::WORDS_ROWS as f32 * line <= PHRASE_BOX_H - 2.0 * 9.0,
            "the word grid's rows overflow the {PHRASE_BOX_H}px seed box",
        );

     
        assert_eq!(compact::WORDS_COLS * compact::WORDS_ROWS, TARGET_WORDS);
        assert!(compact::WORD_CHARS >= 8, "BIP39's longest words are 8 characters");
        assert!(GRID_PLACEHOLDER.chars().count() <= compact::WORD_CHARS);

    
        assert!(
            PHRASE_PLACEHOLDER.chars().count() <= compact::GRID_SPAN,
            "the placeholder is cut off in the empty box",
        );
    }

    
    #[test]
    fn sign_with_matches_the_dashboard_wallet_panes() {
        use crate::ui::components::grid::KEY_PLACEHOLDER;
        for pane in [
            include_str!("../managexrp/panes/wallet.rs"),
            include_str!("../managebtc/panes/wallet.rs"),
        ] {
            assert!(
                pane.contains(&format!("const STANDARD_SIGN: &str = \"{KEY_PLACEHOLDER}\";")),
                "a wallet pane promises a key the signing box does not ask for",
            );
            assert!(
                pane.contains(&format!("const COLD_SIGN: &str = \"{COLD_SIGN}\";")),
                "a wallet pane's COLD_SIGN has drifted from import's cold row",
            );
        }
    }

  
    #[test]
    fn the_type_cards_fit_the_band_below_the_phrase_box() {
   
        let eyebrow = compact::EYEBROW * 1.3 + 8.0;
        let status = compact::ROW * 1.6;
        let spent = eyebrow + PHRASE_BOX_H + 9.0 + status;
        let band = PANES_H - 2.0 * PANE_PAD_V - spent;
        let picker = eyebrow + TYPE_CARD_H;

        assert!(
            picker <= band,
            "the {picker}px type picker overruns the {band}px band left in a {PANES_H}px pane",
        );
     
        assert!(band - picker >= 20.0, "the type picker leaves only {}px of band", band - picker);
        assert!(PANE_FIG <= band, "the {PANE_FIG}px pane figure overruns the {band}px band");
        assert!(band - PANE_FIG <= 30.0, "the pane figure leaves {}px of band — the gap is back", band - PANE_FIG);
    }

 
    #[test]
    fn every_crack_reading_fits_the_compact_row() {
        let budget = ((RIGHT_W - 2.0 * PANE_PAD_H - compact::METER_KEY_W)
            / (compact::METER * tui::MONO_ADVANCE))
            .floor() as usize;
        for kdf in [entropy::Kdf::Argon2id, entropy::Kdf::Bip39] {
            for tenths in 0..=4000 {
                let (runs, _) =
                    compact::crack_runs(tenths as f64 / 10.0, kdf, &theme::COMPACT_OBSIDIAN);
                let n = tui::n_chars(&runs);
                assert!(
                    n <= budget,
                    "{kdf:?} at {} bits draws {n} columns in a {budget} budget: {runs:?}",
                    tenths as f64 / 10.0,
                );
            }
        }
    }


    #[test]
    fn the_one_screen_gate_composes_all_three() {
        let valid = SecureString::new(
            "abandon abandon abandon abandon abandon abandon abandon abandon \
             abandon abandon abandon abandon abandon abandon abandon abandon \
             abandon abandon abandon abandon abandon abandon abandon art"
                .to_string(),
        );
        let swapped = SecureString::new(valid.as_str().replace("art", "zoo"));
        let empty = SecureString::new(String::new());
        let word = SecureString::new("w".repeat(WORD25_MIN));
        let key = SecureString::new("k".repeat(KEY_MIN));
        let short = SecureString::new("k".repeat(KEY_MIN - 1));

        use ImportMode::{Cold, Standard};
        assert!(can_finish(SetupFlow::Import, &valid, false, &empty, Standard, &key));
        assert!(can_finish(SetupFlow::Import, &valid, true, &word, Cold, &empty));
        assert!(!can_finish(SetupFlow::Import, &swapped, false, &empty, Standard, &key));
        assert!(!can_finish(SetupFlow::Import, &empty, false, &empty, Standard, &key));
        assert!(!can_finish(SetupFlow::Import, &valid, true, &empty, Standard, &key));
        assert!(!can_finish(SetupFlow::Import, &valid, false, &empty, Standard, &short));
        assert!(can_finish(SetupFlow::Create, &swapped, false, &empty, Cold, &empty));
        assert!(!can_finish(SetupFlow::Create, &empty, false, &empty, Cold, &empty));
    }


    #[test]
    fn the_word25_gate_refuses_an_empty_yes() {
        let word = |s: &str| SecureString::new(s.to_string());
        assert!(can_continue_word25(false, &word("")));
        assert!(can_continue_word25(false, &word("x")));
        assert!(!can_continue_word25(true, &word("")));
        assert!(!can_continue_word25(true, &word("1")));
        assert!(!can_continue_word25(true, &word("12345")));
        assert!(can_continue_word25(true, &word("123456")));
        assert!(!can_continue_word25(true, &word("      ")));
        assert!(!can_continue_word25(true, &word("  12345  ")));
        assert!(can_continue_word25(true, &word("12 345")));
    }

   
    #[test]
    fn the_checksum_gate_tells_a_phrase_from_a_typo() {
        let valid = "abandon abandon abandon abandon abandon abandon abandon abandon \
                     abandon abandon abandon abandon abandon abandon abandon abandon \
                     abandon abandon abandon abandon abandon abandon abandon art";
        assert!(mnemonic_checksum_ok(valid));
        assert!(mnemonic_checksum_ok(&format!("  {valid}\n")), "whitespace broke the parse");
        let swapped = valid.replace("art", "zoo");
        assert!(!mnemonic_checksum_ok(&swapped));
        assert!(!mnemonic_checksum_ok(""));
    }
}
