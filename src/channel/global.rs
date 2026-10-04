use crate::channel::{Health, CHANNEL};
use crate::secure::SecureString;

// ── Activity Log ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum ActivityStepState {
    Pending,
    Active { since: std::time::Instant },
    Ok { ms: u64, at: std::time::Instant },
    Error { message: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ActivityStep {
    pub id: &'static str,
    pub label: &'static str,
    pub state: ActivityStepState,
}

impl ActivityStep {
    pub fn pending(id: &'static str, label: &'static str) -> Self {
        Self { id, label, state: ActivityStepState::Pending }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ActivityLogState {
    pub title: &'static str,
    pub steps: Vec<ActivityStep>,
    /// One sentence about what the work actually DID, set when the last step
    /// lands. The steps say how far a flow got; they cannot say that an order
    /// filled 612 of 1,000 — and under an immediate-or-cancel contract that
    /// shortfall is the routine outcome, not the exceptional one. Absent for
    /// every flow whose completion carries no facts of its own.
    pub note: Option<String>,
    /// The wire key `begin` gated on. Its namespace names the link this flow's
    /// answer must arrive over, so the log can say "link down" about the right
    /// service. `None` for a log opened without a gate.
    pub health_key: Option<&'static str>,
}

impl ActivityLogState {
    pub fn new(title: &'static str, steps: &[(&'static str, &'static str)]) -> Self {
        Self {
            title,
            steps: steps.iter().map(|(id, label)| ActivityStep::pending(id, label)).collect(),
            note: None,
            health_key: None,
        }
    }

    /// Open a flow's log on its first step, refusing straight away when the
    /// network that flow needs is already known to be down.
    ///
    /// Every transaction flow began the same three lines — build the log, start
    /// `init`, publish — and then went off to spend an argon2id and a round trip
    /// before anything noticed there was no link. The check belongs *in* those
    /// three lines: `Initializing` is the step that means "can this go anywhere
    /// at all", it is the last question answerable for free, and putting it here
    /// means every consumer of the activity log inherits it rather than
    /// remembering to ask.
    ///
    /// The refusal is reported the way every other failure in these flows is —
    /// the active step goes red in the log the user is already watching. No
    /// screen anywhere draws a second copy of it.
    ///
    /// `health` folds the socket into the server's own report, so one key covers
    /// a dead link and a dead node alike. Only an explicit [`Health::Down`]
    /// stops anything: `Unknown` — the component not yet in the map, which is
    /// any attempt made before the first health frame lands — goes through. This
    /// exists to skip a round trip that cannot work, not to arbitrate what the
    /// network will accept.
    pub fn begin(
        title: &'static str,
        steps: &[(&'static str, &'static str)],
        health_key: &'static str,
    ) -> Result<Self, String> {
        let mut log = Self::new(title, steps);
        log.health_key = Some(health_key);
        let Some((first, _)) = steps.first() else {
            return Ok(log);
        };
        log.start(first);
        let _ = CHANNEL.activity_tx.send(Some(log.clone()));

        if CHANNEL.health(health_key) == Health::Down {
            let msg = "No connection to the network — nothing was sent.".to_string();
            log.fail(first, msg.clone());
            let _ = CHANNEL.activity_tx.send(Some(log));
            return Err(msg);
        }
        Ok(log)
    }

    pub fn start(&mut self, id: &'static str) {
        if let Some(s) = self.steps.iter_mut().find(|s| s.id == id) {
            s.state = ActivityStepState::Active { since: std::time::Instant::now() };
        }
    }

    pub fn finish(&mut self, id: &'static str) {
        if let Some(s) = self.steps.iter_mut().find(|s| s.id == id) {
            let ms = match s.state {
                ActivityStepState::Active { since } => since.elapsed().as_millis() as u64,
                _ => 0,
            };
            s.state = ActivityStepState::Ok { ms, at: std::time::Instant::now() };
        }
    }

    pub fn fail(&mut self, id: &'static str, message: String) {
        if let Some(s) = self.steps.iter_mut().find(|s| s.id == id) {
            s.state = ActivityStepState::Error { message };
        }
    }

    pub fn all_ok(&self) -> bool {
        self.steps.iter().all(|s| matches!(s.state, ActivityStepState::Ok { .. }))
    }

    pub fn any_error(&self) -> bool {
        self.steps.iter().any(|s| matches!(s.state, ActivityStepState::Error { .. }))
    }

    pub fn is_terminal(&self) -> bool {
        self.all_ok() || self.any_error()
    }

    /// State what happened, alongside the step marks. Does not change whether
    /// the log reads as done or failed — that stays the steps' business, so a
    /// partial fill can be reported in full without being dressed as an error.
    pub fn note(&mut self, message: String) {
        self.note = Some(message);
    }

    pub fn fail_active(&mut self, message: String) {
        for step in self.steps.iter_mut() {
            if matches!(step.state, ActivityStepState::Active { .. }) {
                step.state = ActivityStepState::Error { message };
                break;
            }
        }
    }

    /// State a failure on whichever step is carrying it — the one still
    /// running, or the one that has already failed.
    ///
    /// The second case is the whole reason this exists. The watchdog fails a
    /// step with an honest "it may still complete", and up to a
    /// minute later the ledger index passes the transaction's
    /// `LastLedgerSequence` and turns that doubt into a fact. Refusing to
    /// overwrite would leave the weaker of two true statements on screen for
    /// no reason other than which arrived first.
    pub fn restate_failure(&mut self, message: String) {
        for step in self.steps.iter_mut() {
            if matches!(
                step.state,
                ActivityStepState::Active { .. } | ActivityStepState::Error { .. }
            ) {
                step.state = ActivityStepState::Error { message };
                return;
            }
        }
    }
}



// NOT `Clone`: the three secret fields hold mlocked `SecureString`s, so a clone
// would duplicate secret bytes into a second locked buffer. Consumers move the
// command (across the mpsc channel) and `.take()` the secrets out for auth.
#[derive(Debug, Default)]
pub struct WSCommand {
    pub command: String,
    pub wallet: Option<String>,
    pub recipient: Option<String>,
    pub amount: Option<String>,
    pub passphrase: Option<SecureString>,
    pub trustline_limit: Option<String>,
    pub fee: Option<String>,
    pub tx_type: Option<String>,
    pub taker_pays: Option<(String, String)>,
    pub taker_gets: Option<(String, String)>,
    pub seed: Option<SecureString>,
    pub flags: Option<Vec<String>>,
    pub wallet_type: Option<String>,
    pub bip39: Option<SecureString>,
    pub offer_sequence: Option<u32>,
    /// Manually-entered XRP destination tag (r-address path only). For an
    /// X-address recipient the tag is decoded from the address and this is
    /// ignored. None means "no tag".
    pub destination_tag: Option<u32>,
    /// A BTC address list. Since 2026-09-20 its one use is the wallet's LIVE
    /// list on `subscribe_bitcoin_addresses` (the import's address window it
    /// was named for is gone — the account xpub rides the import instead).
    /// Addresses only — public data, no secrets ride here.
    pub scan: Option<Vec<String>>,
    /// Fee bump: the mempool txid the signed replacement supersedes — the
    /// relay marks it `replaced` on an accepted broadcast.
    pub replaces: Option<String>,
    /// A history page (`get_history` / `get_bitcoin_history`): which list, in
    /// the relay's word (XRP only — Bitcoin has one list), and how many
    /// settled rows of it the app already holds.
    pub history_kind: Option<&'static str>,
    pub history_offset: Option<usize>,
}
