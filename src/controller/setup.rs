use rand::seq::{index, SliceRandom};
use rand::RngExt;

use crate::controller::app_state::{AppState, PhraseCheck, SetupChain, SetupTask};
use crate::utils::phrase;

pub const CHECK_SLOTS: usize = 3;
pub const CHECK_CHOICES: usize = 4;

pub fn reset(state: &mut AppState, chain: SetupChain) {
    match chain {
        SetupChain::Xrp => state.xrp_setup_task = SetupTask::default(),
        SetupChain::Btc => state.btc_setup_task = SetupTask::default(),
    }
    state.create_check = None;
    state.create_checked = false;
    state.create_mismatch = None;
    state.phrase_line = Default::default();
}

pub fn open(state: &mut AppState, chain: SetupChain, task: SetupTask) {
    let open = match chain {
        SetupChain::Xrp => &mut state.xrp_setup_task,
        SetupChain::Btc => &mut state.btc_setup_task,
    };
    if *open != task {
        *open = task;
        state.create_check = None;
        state.create_mismatch = None;
    }
}

pub fn start_check(state: &mut AppState) {
    let words: Vec<&str> = state.create_mnemonic.as_str().split_whitespace().collect();
    if words.len() != phrase::SLOTS || state.create_checked {
        return;
    }
    let mut rng = rand::rng();
    let mut positions = [0usize; CHECK_SLOTS];
    for (slot, p) in positions.iter_mut().zip(index::sample(&mut rng, phrase::SLOTS, CHECK_SLOTS).iter()) {
        *slot = p;
    }
    positions.sort_unstable();

    let list = phrase::list();
    let mut options = [[""; CHECK_CHOICES]; CHECK_SLOTS];
    for (row, &p) in options.iter_mut().zip(positions.iter()) {
        let Some(right) = phrase::canonical(words[p]) else { return };
        let mut set = Vec::with_capacity(CHECK_CHOICES);
        set.push(right);
        while set.len() < CHECK_CHOICES {
            let w = list[rng.random_range(0..list.len())];
            if !words.contains(&w) && !set.contains(&w) {
                set.push(w);
            }
        }
        set.shuffle(&mut rng);
        row.copy_from_slice(&set);
    }
    state.create_check = Some(PhraseCheck { positions, options, answered: 0 });
    state.create_mismatch = None;
}

pub fn pick(state: &mut AppState, choice: usize) {
    let Some(check) = state.create_check.as_mut() else { return };
    let (Some(&p), Some(&picked)) = (
        check.positions.get(check.answered),
        check.options.get(check.answered).and_then(|o| o.get(choice)),
    ) else {
        return;
    };
    if state.create_mnemonic.as_str().split_whitespace().nth(p) != Some(picked) {
        state.create_check = None;
        state.create_mismatch = Some(p);
        return;
    }
    check.answered += 1;
    if check.answered == CHECK_SLOTS {
        state.create_check = None;
        state.create_checked = true;
        state.create_seed_reveal = false;
    }
}
