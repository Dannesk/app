//! Shared UI for the wallet import/create flows (XRP + BTC) and the
//! key-management restore flow. The flows keep their own state (separate
//! mlocked secret buffers — structural isolation), but their *rendering* is
//! identical, so it lives here.
//!
//! **Import and create are ONE screen each now** — [`setup_screen`], on the
//! compact spec (import v2, 2026-08-31): a cardless two-pane frame divided by
//! a single hairline. The left pane is the one thing you bring (the typed
//! phrase, or the generated grid); the right pane is the two questions you
//! answer (25th word, key storage) and the submit. No steps, no progress
//! mark, no icons. Under the box, BTC's left pane carries the address-type
//! cards and XRP's a small copy of the gate's orbit ([`PANE_FIG`]) — the
//! cards took the band from the orbit on BTC on 2026-09-14; before the orbit
//! a reassurance paragraph sat there. Primitives live in
//! [`super::compact`]; the palette is
//! [`theme::compact`] — no brand blue anywhere on this surface.
//!
//! **The 3-step card surface that used to live below is DELETED** (2026-09-01).
//! It existed for restore, and dashboard v3 moved restore into
//! [`super::key_stack`]'s panel faces, which left `phrase_step`, `word25_step`,
//! `backline`, `hero`/`hero_icon`/`card`/`card_stage`/`cta`/`radio`/
//! `radio_card`/`cap_note`/`strength_block`/`dot` and `SetupFlow::Restore` with
//! no caller at all.
//!
//! What survives is the shared vocabulary the OTHER surfaces still borrow —
//! [`underline_field`], [`phrase_well`], [`eye`], [`text_link`], [`mono_runs`],
//! [`checksum_verdict`] and the geometry consts behind them, which
//! `review_sign` and the signing screens draw with. That is the whole reason
//! this module is not simply the compact screen.
//!
//! **Beware when pruning here again:** `pub` items in a binary crate do NOT
//! raise `dead_code`, so the compiler will not tell you when something goes
//! cold. The way to find out is to strip the visibility keywords, let the
//! access errors name the real external surface, restore only those, and read
//! the warnings that fall out.

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

/// Whether `phrase` is a BIP39 mnemonic with a valid checksum.
///
/// Read-only: nothing is derived from it, no key material leaves the check —
/// the bridge re-parses at submit exactly as before. This exists so step 1 can
/// say "invalid checksum" next to the field instead of letting a typo travel
/// to step 2 and fail there.
pub fn mnemonic_checksum_ok(phrase: &str) -> bool {
    bip39::Mnemonic::parse_in(bip39::Language::English, phrase.trim()).is_ok()
}

// ── Strength vocabulary ─────────────────────────────────────────────────────

/// Words in a recovery phrase. Not a knob — the derivation and the counter
/// assume it.
pub(crate) const TARGET_WORDS: usize = 24;

/// What the ENCRYPTION gauge should read: the live buffer, or the value held
/// across a submit that emptied it.
///
/// The hold applies **only while the buffer is empty**, which is what makes it
/// self-releasing. A submit moves the key out to the bridge and the gauge keeps
/// its last reading instead of collapsing to a red `0 bits` under the user's
/// hands; a failure that leaves them retyping gets the live measurement back on
/// the first keystroke. An untouched step 2 has no hold to find and reads zero,
/// exactly as it always did.
pub(crate) fn held_bits(secret: &SecureString, hold: Option<f64>) -> f64 {
    if secret.char_len() == 0 {
        hold.unwrap_or(0.0)
    } else {
        entropy::bits(secret.as_str().trim())
    }
}

/// While the phrase is short of 24 words there is no checksum to report — the
/// counter row above is already saying how far along it is.
const CHECKSUM_PENDING: &str = "awaiting all 24 words";
const CHECKSUM_VALID: &str = "valid checksum";
const CHECKSUM_INVALID: &str = "invalid checksum";

pub(crate) const PHRASE_PLACEHOLDER: &str = "paste or type your 24 words…";
/// The header's shortcuts on the typed flows, key then action. Clicking a
/// cell needs no label — it is the thing people try first — but paste and
/// Enter are found by chance otherwise. Space still separates words, as any
/// typist expects, but it is not the advertised way on: done with a word,
/// you press Enter.
const PHRASE_KEYS: &[(&str, &str)] = &[("ctrl+v", "paste"), ("enter", "next word")];
/// The create flow's: the phrase is read, not driven, so only paste — for
/// the 25th word and the key on the right.
const CREATE_KEYS: &[(&str, &str)] = &[("ctrl+v", "paste")];

/// Characters a Standard-mode key must reach — the number [`can_submit`] gates
/// on, read from one place so the row can't promise a length the button
/// disagrees with.
///
/// **Six of anything, not six digits.** This was fifteen, which was a policy
/// dressed as a limit: it declared how much risk the user was permitted to
/// accept with their own money. The ENCRYPTION rows under the field now state
/// the cost of the choice in bits, time and dollars, so the screen informs
/// instead of forbidding — six digits reads back as *$2 of rented compute*,
/// which is a far more effective argument than a disabled button, and an
/// honest one.
///
/// A minimum survives at all only because zero-length is a mistake rather than
/// a choice, and because a wallet made here must stay re-encryptable: this is
/// also the floor [`crate::ui::components::signing::PASSPHRASE_MIN`] enforces at
/// signing time, and a key that could be stored but not typed back would be a
/// wallet locked by its own setup screen.
pub const KEY_MIN: usize = 6;

/// Which flow a shared step is drawing for — the copy differs, nothing else.
/// `Restore` is the key-management restore flow, which is importing without
/// the backend call: its steps 1–2 are these exact components, with the copy
/// flipped to past tense because the answers were fixed at import.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetupFlow {
    Import,
    Create,
}

// ── Step 2: the 25th word (restore's, and the compact screen's gate) ────────

/// Shortest 25th word setup accepts, counted after the ends are trimmed —
/// see [`SecureString::take_trimmed`]. A word this short adds nothing an attacker notices, and a
/// shorter one is far more often a leftover — `12346` backspaced to `1` by
/// someone who changed their mind — than a choice. That leftover still derives
/// a valid wallet, one they will not be able to reach again.
pub const WORD25_MIN: usize = 6;

/// Whether step 2 may advance: `No`, or `Yes` with at least [`WORD25_MIN`]
/// characters once the ends are trimmed. `Yes` and an empty field is someone
/// who forgot, and silently proceeding as `No` is the one thing that derives
/// the wrong wallet. Counted exactly as the field is trimmed when it is taken,
/// so a field of only spaces cannot pass for a word.
pub fn can_continue_word25(has_word: bool, bip39: &SecureString) -> bool {
    !has_word || bip39.trimmed_char_len() >= WORD25_MIN
}

/// The 25th-word field's placeholder, per flow.
///
/// Import says exactly what every signing form says
/// ([`super::grid::BIP39_PLACEHOLDER`]) — it used to explain itself, `only
/// if your existing wallet has one`, which is a sentence where the rest of
/// the app has two words, and it is the docs' job to say what a 25th word
/// is (user, 2026-09-16). Someone importing a wallet either set one or did
/// not; nobody learns that from a placeholder.
///
/// Create keeps its own, because there the word is being CHOSEN and the
/// [`WORD25_MIN`] floor is a rule the field enforces — advice the box has to
/// give before the Continue goes dark for a reason nothing names.
fn word25_placeholder(flow: SetupFlow) -> &'static str {
    match flow {
        SetupFlow::Import => super::grid::BIP39_PLACEHOLDER,
        SetupFlow::Create => "optional 25th word - 6 minimum characters",
    }
}

/// Whether the storage secret is acceptable for the selected mode: Standard
/// wants a real key (≥ [`KEY_MIN`] once the ends are trimmed, matching the
/// signing floor so a wallet made here can always be re-encrypted later); Cold
/// stores nothing.
fn can_submit(mode: ImportMode, secret: &SecureString) -> bool {
    match mode {
        ImportMode::Standard => secret.trimmed_char_len() >= KEY_MIN,
        ImportMode::Cold => true,
    }
}

// ── The compact one-screen setup (import v2) ────────────────────────────────
//
// The three steps collapsed onto one 640-wide frame: a header strip over two
// cardless panes divided by a single `rule` hairline. Left pane = the one
// thing you bring (the typed phrase on import, the generated grid on create);
// right pane = the two questions you answer (25th word · key storage) and the
// submit. Both flows, both chains, one assembly — the chain adapters
// (`managexrp::xrpsetup`, `managebtc::btcsetup`) pass their own buffers and
// messages, exactly as the step wrappers used to.
//
// **This screen draws no errors.** Submitting runs the import/create bridge,
// which opens an `ActivityLogState` and fails there on every path that can go
// wrong. [`can_finish`] is what makes that safe — nothing reaches the bridge
// without 24 checksummed words, an answered 25th word, and a key of at least
// [`KEY_MIN`] (or Cold) — and the controller re-checks the same gate on
// submit, because with the steps gone there is no navigation in front of it.

/// The frame: header strip + two panes. 403 + 1 (divider) + 396 = 800.
///
/// Widened from 640 (2026-09-03) so every full-window content screen presents
/// the same block: this, send v3 and both dashboards are all 800 now, which is
/// also what Settings' two columns come to. `scale()` could not deliver this —
/// it is height-bound at every realistic viewport and uniform, so it would have
/// grown the frame vertically too. [`PANES_H`] is untouched.
///
/// The +160 is split in the panes' existing proportion. Receive, the signing
/// stacks, Tokens and the activity log deliberately did NOT follow: none of
/// them is a big-content screen, and widening them would just spread thin
/// content over more pixels.
const FRAME_W: f32 = 800.0;
const LEFT_W: f32 = 403.0;
/// Was the mock's 318 less the divider it carried as a border.
const RIGHT_W: f32 = 396.0;
const PANE_PAD_V: f32 = 12.0;
const PANE_PAD_H: f32 = 16.0;

/// The panes' shared height — the mock's `align-items: stretch`, resolved to a
/// constant so the divider can run the full block without a measuring pass.
/// Sized to the tallest state (device mode + a 25th word, both fields and both
/// meter pairs open); the short states leave their slack below the CTA,
/// exactly as the mock does.
///
/// **This is why the key-storage block is capped at two stat rows.** A third
/// (`Sign with`, tried 2026-09-12) pushed the tallest state past the fixed
/// height and clipped the `Import wallet` CTA against the pane's bottom edge.
/// One `stat_rows` row costs exactly 16 — `ROW` 10 at iced's default 1.3
/// leading = 13, plus the list's 3px spacing — so a fourth row means +16 here
/// and the user chose two-and-two over a taller pane.
///
/// Add anything to that state and this constant owes you its height;
/// `the_type_cards_fit_the_band_below_the_phrase_box` fails if you change it
/// without re-checking the left pane.
const PANES_H: f32 = 386.0;

/// The seed box, both flows: fixed, so the resting shape and the filled one
/// are the same shape. Holds the 8-row word grid (8 × 16px rows + padding ≈
/// 146) that BOTH faces draw — import's editable cells and create's read-only
/// one share [`compact`]'s grid constants.
const PHRASE_BOX_H: f32 = 154.0;

/// What an empty cell shows before generation: a quiet placeholder, so the
/// grid states its shape — 24 slots, waiting — without pretending to hold
/// anything.
const GRID_PLACEHOLDER: &str = "\u{b7}\u{b7}\u{b7}\u{b7}\u{b7}";

/// The provenance line under the grid. It can say "this device" because
/// generation happens on this screen, from the user's own click.
const NEVER_SENT: &str = "generated on this device \u{2014} never sent anywhere";
const NOT_YET: &str = "nothing generated yet";

/// The address-type cards' height, and the two type sizes on a card: the
/// prefix (mono) over the name. Eyebrow + cards ≈ 19 + 52 of the ~164px band
/// the box and status row leave at [`PANES_H`]; the picker centres in the
/// rest. It took the band over from the small orbit that used to fill it
/// (2026-09-12 → 2026-09-14): the gate already shows the orbit, and a
/// control earns the space a filler only occupied.
const TYPE_CARD_H: f32 = 52.0;
const TYPE_CARD_PREFIX: f32 = 14.0;
const TYPE_CARD_NAME: f32 = 9.0;

/// The pane figure for a chain with no picker (XRP): the no-wallet gate's
/// orbit, small, centred in the same band — 140 of [`wallet_gate::FIG`]'s
/// 376, which leaves about 12 either side at [`PANES_H`] and cannot overrun
/// the fixed pane height, since neither left pane grows. Restored for XRP on
/// 2026-09-14 after a day without it: with no cards, the band was a hole.
const PANE_FIG: f32 = 140.0;

/// Cold storage, stated as our mechanism actually works: the seed is asked for
/// at **signing**, not at launch — the wallet itself opens watch-only. (The
/// mock said "each time the wallet opens", which is not what this app does.)
const COLD_NOTE: &str = "Your seed is never written to disk. You type your 24 words \
each time you sign.";

/// What each mode asks for at signing time, in the `Sign with` stat row.
///
/// It replaced Cold's `Re-entry · every signing` (2026-09-12): "re-entry"
/// named the mechanism from the app's side and said nothing about what the
/// user would be asked to produce. Standard gained the same row — the two
/// modes differ in exactly this, so stating it on one and not the other left
/// the comparison half-made.
///
/// These are the strings the dashboard wallet panes' `sign with` row already
/// uses (`managexrp/panes/wallet.rs`, `managebtc/panes/wallet.rs`), so what
/// setup promises and what the wallet later reports back are one vocabulary.
/// Keep the three in step — they are deliberately not shared, because the
/// panes are the chain-local twins that must not be merged.
const COLD_SIGN: &str = "24 word mnemonic";

/// The one gate the compact screen submits through: a complete phrase (with
/// its checksum on import — a generated phrase is valid by construction), an
/// answered 25th word, and a storable key. The same functions the step
/// buttons used to gate on, composed — the controller calls this too, since
/// no step navigation stands in front of the bridge any more.
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

/// Everything one chain's flow hands the shared screen. Concrete messages
/// rather than closures, so the screen stays ignorant of which chain — the
/// same shape [`crate::ui::components::key_stack::KeyStack`] uses.
pub struct SetupScreenParams<'a> {
    /// `Import` or `Create` — decides the left pane. `Restore` never routes
    /// here; it keeps the stepped surface below.
    pub flow: SetupFlow,
    /// The header strip: `Import wallet` / `Create wallet`.
    pub title: &'static str,
    /// The derivation path this chain walks from the phrase, shown ONCE, in
    /// the recovery-phrase eyebrow's right slot beside the word count. It
    /// replaced `bip-39` there (2026-09-03): both chains' phrases satisfy
    /// BIP-39, so that label distinguished nothing, while the path is
    /// chain-specific and sits beside the exact words it applies to. It was
    /// on the no-wallet gate first, under each row, and came off as redundant.
    pub path: &'static str,
    /// Which mark the pane figure's plate carries — the same enum the
    /// no-wallet gate takes, so a chain names its mark in one vocabulary.
    /// Drawn only for a chain with no address types (XRP).
    pub mark: Mark,
    /// The address-type picker under the phrase box — BTC only; XRP passes
    /// an empty list and gets the pane figure instead. Each entry is a
    /// card: the address prefix, the type's name, whether it is the current
    /// choice, and the message that chooses it. It takes the band under the
    /// status row that the figure fills on the other chain (2026-09-14).
    pub address_types: Vec<(&'static str, &'static str, bool, Message)>,
    /// The eyebrow's right slot — the current type's BIP. Ignored when
    /// `address_types` is empty.
    pub address_type_bip: &'static str,
    pub seed_field: SecureField,
    pub seed: &'a SecureString,
    pub seed_reveal: bool,
    /// Create only: the `Copy ›` feedback beat.
    pub copied: bool,
    /// Create only.
    pub on_generate: Option<Message>,
    /// Create only.
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

/// The complete one-screen import/create surface.
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

    // The back chevron floats at the window's top-left; the frame centers in
    // the whole stage, so the two never fight for a row.
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
    .into()
}

/// [`checksum_verdict`] in the compact ink — same words, same rules, the
/// colours from the compact ramp.
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

/// The import left pane: eyebrow · phrase box · status row · pinned note.
fn import_pane<'a>(
    prm:   &SetupScreenParams<'a>,
    cp:    &'static theme::CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    let words = prm.seed.as_str().split_whitespace().count();
    let complete = words == TARGET_WORDS;
    let checksum_ok = complete && mnemonic_checksum_ok(prm.seed.as_str());

    // Faint at zero, amber part-way, then the checksum's own verdict at 24 —
    // a full count with a bad word is a failure, not an achievement.
    let counter_colour = match words {
        0 => cp.faint,
        n if n == TARGET_WORDS => {
            if checksum_ok { cp.green } else { cp.red }
        }
        _ => cp.amber,
    };

    let verdict: Element<'a, Message> = if words == 0 {
        Space::new().into()
    } else {
        let (body, colour) = compact_checksum_verdict(complete, checksum_ok, cp);
        let glyph = match (complete, checksum_ok) {
            (true, true) => "\u{2713} ",
            (true, false) => "\u{2715} ",
            _ => "",
        };
        text(format!("{glyph}{body}")).size(compact::ROW * scale).color(colour).into()
    };

    // `Paste all ›` on an empty box, `Clear ›` on a full one: a paste onto a
    // half-typed phrase replaces it, so the two never both apply.
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
    // Fill, not the default Shrink: the pane container is fixed-height, and a
    // `Fill` child inside a Shrink column collapses to nothing — the picker
    // would have no band to centre in.
    .height(Length::Fill)
    .into()
}

/// The address-type picker (2026-09-14): an eyebrow naming the choice's
/// BIP, then one card per type the flow offers — the prefix people know a
/// wallet by over the type's name — centred in the band the box and status
/// row leave. A chain that passes no types (XRP: one path, one address, no
/// choice to make) gets the pane figure in the band instead — the small
/// orbit that filled it on both chains before the cards existed.
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
            .into();
    }
    let mut cards = row![].spacing(6.0 * scale).width(Length::Fill);
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
    .into()
}

/// One picker card: the prefix in mono over the name, the segmented
/// control's fill and ink at card height.
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
    .into()
}

/// The create left pane: the same box, holding the generated grid — 24 slots
/// before generation, 24 words after, always full height. Generation happens
/// on this screen from the user's own `Generate ›`, so there is no mask and
/// no eye; one roll per flow, and once words exist the link is gone for good.
/// `Copy ›` stays — the clipboard is a bad place for a phrase, but convenience
/// over security is the user's call to make, not ours.
fn create_pane<'a>(
    prm:   &SetupScreenParams<'a>,
    cp:    &'static theme::CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    let words: Vec<&str> = prm.seed.as_str().split_whitespace().collect();
    let generated = words.len() == TARGET_WORDS;

    let mut grid = column![].spacing(3.5 * scale);
    for r in 0..compact::WORDS_ROWS {
        let mut runs: Vec<Run> = Vec::new();
        for c in 0..compact::WORDS_COLS {
            if c > 0 {
                runs.push((" ".repeat(compact::GAP_CHARS), cp.text));
            }
            // Column-major: 01–08 down the first column, like import's grid.
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
            .into()
    } else {
        text(NOT_YET).size(compact::ROW * scale).color(cp.muted).into()
    };

    let action: Element<'a, Message> = if !generated {
        compact::upper_link(
            "Generate",
            prm.on_generate.clone().expect("create pane without on_generate"),
            cp,
            scale,
        )
    } else if prm.copied {
        text("\u{2713} copied").size(compact::ROW * scale).color(cp.green).into()
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
    // See import_pane: Fill so the picker has a band to centre in.
    .height(Length::Fill)
    .into()
}

/// The keys pane: the `25th word` section, a hairline, the `key storage`
/// section, and the submit. Radio cards became 2-up segments; each answer
/// opens its field with the same live meter rows the stepped flow drew.
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
            .push(Space::new().height(8.0 * scale))
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
            .push(Space::new().height(8.0 * scale))
            // Priced against BIP39's own stretch, not ours: the word is never
            // stored here, so the only attacker already holds the 24 words.
            .push(compact::meter_rows(
                entropy::bits(prm.w25.as_str().trim()),
                entropy::Kdf::Bip39,
                cp,
                scale,
            ));
    }

    col = col
        .push(Space::new().height(12.0 * scale))
        .push(compact::hairline(cp.rule))
        .push(Space::new().height(11.0 * scale))
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
                .push(Space::new().height(8.0 * scale))
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
                .push(Space::new().height(8.0 * scale))
                .push(compact::meter_rows(
                    held_bits(prm.enc, state.entropy_hold),
                    entropy::Kdf::Argon2id,
                    cp,
                    scale,
                ))
                // Verified against `encrypt.rs`: Aes256Gcm over an Argon2id
                // (64 MB, t=3, p=4) key. If the crypto ever changes, this
                // block is lying until it changes too.
                .push(compact::stat_rows(
                    &[("Cipher", "AES-256-GCM"), ("KDF", "Argon2id")],
                    cp,
                    scale,
                ));
        }
        ImportMode::Cold => {
            col = col
                .push(Space::new().height(8.0 * scale))
                .push(
                    text(COLD_NOTE)
                        .size(10.5 * scale)
                        .line_height(iced::widget::text::LineHeight::Relative(1.5))
                        .color(cp.dim),
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
        .push(Space::new().height(13.0 * scale))
        .push(compact::cta(prm.submit_label, enabled, prm.on_submit.clone(), cp, scale));

    col.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The gauge holds its reading across a submit, and lets go of it again on
    /// the first keystroke of a retry.
    ///
    /// The hold exists because every storage surface MOVES its key out to the
    /// bridge: without it the bar fell from green to a danger-red `0 bits` at
    /// the moment of commitment, which reads as a verdict rather than as an
    /// emptied buffer.
    #[test]
    fn a_held_reading_survives_submit_but_not_a_retry() {
        let typed = SecureString::new("correct horse battery staple".to_string());
        let empty = SecureString::new(String::new());
        let live = entropy::bits(typed.as_str());

        // Nothing in flight: the gauge measures what is in the buffer.
        assert_eq!(held_bits(&typed, None), live);
        // Untouched step — no hold to find, and zero is the honest reading.
        assert_eq!(held_bits(&empty, None), 0.0);
        // Submitted: the buffer moved out, the reading stays where it was.
        assert_eq!(held_bits(&empty, Some(live)), live);
        // Retyping after a failure goes live again, stale hold or not.
        assert_eq!(held_bits(&typed, Some(1.0)), live);
    }

    /// The compact panes' geometry: the 24-word cell grid — import's editable
    /// face and create's read-only one share it — fits the seed box in both
    /// axes, and the box's fixed height holds all eight rows.
    /// The cell width IS the wordlist's longest word: the box refuses a
    /// letter past it, so a wider list would make the box refuse real words.
    #[test]
    fn the_cell_width_is_the_longest_bip39_word() {
        let longest = bip39::Language::English
            .word_list()
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

        // Width: the grid's span inside the box's padding (11 left, and the
        // eye's 30-column-equivalent reserve on the right of import's box).
        let box_inner = LEFT_W - 2.0 * PANE_PAD_H - 11.0 - (11.0 + 13.0 + 6.0);
        assert!(
            compact::GRID_SPAN as f32 * advance <= box_inner,
            "the word grid ({} columns) is wider than the seed box",
            compact::GRID_SPAN,
        );

        // Height: eight cell rows inside the box's vertical padding.
        assert!(
            compact::WORDS_ROWS as f32 * line <= PHRASE_BOX_H - 2.0 * 9.0,
            "the word grid's rows overflow the {PHRASE_BOX_H}px seed box",
        );

        // Shape: 24 cells, a cell wide enough for any BIP39 word, and the
        // create grid's placeholder inside a cell.
        assert_eq!(compact::WORDS_COLS * compact::WORDS_ROWS, TARGET_WORDS);
        assert!(compact::WORD_CHARS >= 8, "BIP39's longest words are 8 characters");
        assert!(GRID_PLACEHOLDER.chars().count() <= compact::WORD_CHARS);

        // The empty state's placeholder line renders un-truncated (the widget
        // cuts it to the field's own columns).
        assert!(
            PHRASE_PLACEHOLDER.chars().count() <= compact::GRID_SPAN,
            "the placeholder is cut off in the empty box",
        );
    }

    /// What a wallet pane says you will sign with is what you are then asked
    /// for — in the same words, on both chains.
    ///
    /// Copies of these two strings live in three files that must not be merged,
    /// so this reads the panes' own source back rather than trusting anyone to
    /// keep them in step by hand. The standard side is pinned to
    /// [`grid::KEY_PLACEHOLDER`] — the signing box's own placeholder — because
    /// that is the promise being kept: the pane says `sign with · encryption
    /// key`, and the box you meet later says `encryption key`. The cold side is
    /// pinned to [`COLD_SIGN`], which import's cold stat row prints.
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

    /// The address-type picker fits the band the phrase box and status row
    /// leave. It is a fixed height inside a fixed-height pane, so nothing at
    /// runtime stops it overrunning [`PANES_H`] and pushing the status row
    /// into the divider's foot — only this. Raise `TYPE_CARD_H` or
    /// `PHRASE_BOX_H` past the slack and the test says so instead of the
    /// screen.
    #[test]
    fn the_type_cards_fit_the_band_below_the_phrase_box() {
        // What the pane spends before the band: the eyebrow (mono EYEBROW on
        // iced's default 1.3 leading, plus its 8px bottom pad), the box, the
        // 9px gap, and the status row (mono ROW at the grid's 1.6 leading —
        // the taller of the two things in that row).
        let eyebrow = compact::EYEBROW * 1.3 + 8.0;
        let status = compact::ROW * 1.6;
        let spent = eyebrow + PHRASE_BOX_H + 9.0 + status;
        let band = PANES_H - 2.0 * PANE_PAD_V - spent;
        let picker = eyebrow + TYPE_CARD_H;

        assert!(
            picker <= band,
            "the {picker}px type picker overruns the {band}px band left in a {PANES_H}px pane",
        );
        // With room to centre in: a picker that fills the band to the pixel
        // reads as jammed against the status row.
        assert!(band - picker >= 20.0, "the type picker leaves only {}px of band", band - picker);
        // And the figure the other chain draws in the same band.
        assert!(PANE_FIG <= band, "the {PANE_FIG}px pane figure overruns the {band}px band");
        assert!(band - PANE_FIG <= 30.0, "the pane figure leaves {}px of band — the gap is back", band - PANE_FIG);
    }

    /// The compact meter's `crack time` row has a fixed budget — the keys
    /// pane's width less the key column, in mono characters at 9.5 — and
    /// every rung the ladder can produce, with its cost, has to fit it.
    /// Swept rather than sampled, because the rungs land at different bits
    /// under each KDF.
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

    /// The one-screen gate composes the three step gates it replaced: no
    /// bridge call without a checksummed phrase, an answered 25th word, and a
    /// storable key — and Cold, as ever, needs no key.
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
        // The happy paths.
        assert!(can_finish(SetupFlow::Import, &valid, false, &empty, Standard, &key));
        assert!(can_finish(SetupFlow::Import, &valid, true, &word, Cold, &empty));
        // Each gate refuses alone.
        assert!(!can_finish(SetupFlow::Import, &swapped, false, &empty, Standard, &key));
        assert!(!can_finish(SetupFlow::Import, &empty, false, &empty, Standard, &key));
        assert!(!can_finish(SetupFlow::Import, &valid, true, &empty, Standard, &key));
        assert!(!can_finish(SetupFlow::Import, &valid, false, &empty, Standard, &short));
        // Create trusts its own generation: count, not checksum.
        assert!(can_finish(SetupFlow::Create, &swapped, false, &empty, Cold, &empty));
        assert!(!can_finish(SetupFlow::Create, &empty, false, &empty, Cold, &empty));
    }

    /// `No` always advances; `Yes` needs a word of at least `WORD25_MIN`
    /// characters, counted after the ends are trimmed.
    #[test]
    fn the_word25_gate_refuses_an_empty_yes() {
        let word = |s: &str| SecureString::new(s.to_string());
        assert!(can_continue_word25(false, &word("")));
        assert!(can_continue_word25(false, &word("x")));
        assert!(!can_continue_word25(true, &word("")));
        assert!(!can_continue_word25(true, &word("1")));
        assert!(!can_continue_word25(true, &word("12345")));
        assert!(can_continue_word25(true, &word("123456")));
        // Spaces at the ends do not count toward the minimum…
        assert!(!can_continue_word25(true, &word("      ")));
        assert!(!can_continue_word25(true, &word("  12345  ")));
        // …spaces between characters do.
        assert!(can_continue_word25(true, &word("12 345")));
    }

    /// The checksum gate accepts a real phrase and rejects a word swap — the
    /// failure it exists to catch at step 1 instead of step 3.
    #[test]
    fn the_checksum_gate_tells_a_phrase_from_a_typo() {
        // The BIP39 test vector for 32 zero bytes.
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
