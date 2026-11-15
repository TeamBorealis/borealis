//! One engine check of a draft at a time (`homebrew_check_draft`, item Isq-Y). An engine check levels a test character
//! 1 to 20 over the loaded content: on a large imported library it takes most of a minute on a phone and holds a
//! content snapshot's worth of engine memory while it runs. The UI asks again after every pause in editing and drops
//! an answer for a form that has changed since (`draftSession.ts`), so checks used to pile up, each running to the
//! end for nothing, together using several cores and enough memory to get the app killed on a phone.
//!
//! Now checks take turns, and a check whose draft was asked to be checked again while it waited for its turn is not
//! run: it answers with [`superseded`] at once (the newer check answers for that draft). A check that has started
//! runs to its end.

use crate::error::ApiError;
use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};

/// The engine checks' turns and the latest request of each draft.
#[derive(Debug, Default)]
pub struct CheckGate {
    turn: Mutex<()>,
    latest: Mutex<HashMap<String, u64>>,
}

/// A check's place: its draft and request number.
#[derive(Debug)]
pub struct CheckTicket {
    draft_id: String,
    number: u64,
}

/// The answer of a check a newer check of its draft replaced before it ran.
#[must_use]
pub fn superseded() -> ApiError {
    ApiError::requirement("A newer check of this draft replaced this one.")
}

impl CheckGate {
    /// Record a request to check `draft_id`: it is that draft's latest from now on.
    ///
    /// # Errors
    /// `internal` when a lock is poisoned.
    pub fn request(&self, draft_id: &str) -> Result<CheckTicket, ApiError> {
        let mut latest = self.latest.lock()?;
        let number = latest.get(draft_id).map_or(0, |n| n + 1);
        latest.insert(draft_id.to_string(), number);
        Ok(CheckTicket {
            draft_id: draft_id.to_string(),
            number,
        })
    }

    /// Wait for the running check to end, then hold the turn; [`superseded`] instead when the ticket's draft was asked
    /// to be checked again meanwhile.
    ///
    /// # Errors
    /// [`superseded`]; `internal` when a lock is poisoned.
    pub fn turn(&self, ticket: &CheckTicket) -> Result<MutexGuard<'_, ()>, ApiError> {
        let turn = self.turn.lock()?;
        if self.latest.lock()?.get(&ticket.draft_id) != Some(&ticket.number) {
            return Err(superseded());
        }
        Ok(turn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn a_check_asked_again_while_it_waits_is_not_run() {
        let gate = CheckGate::default();
        let first = gate.request("d1").unwrap();
        let other = gate.request("d2").unwrap();
        let second = gate.request("d1").unwrap();
        assert!(gate.turn(&first).is_err(), "replaced by the second request");
        assert!(gate.turn(&other).is_ok(), "another draft's check runs");
        assert!(gate.turn(&second).is_ok(), "the latest request runs");
    }

    #[test]
    fn checks_take_turns() {
        let gate = Arc::new(CheckGate::default());
        let a = gate.request("a").unwrap();
        let held = gate.turn(&a).unwrap();
        let waiting = {
            let gate = Arc::clone(&gate);
            std::thread::spawn(move || {
                let b = gate.request("b").unwrap();
                gate.turn(&b).map(|_| ()).is_ok()
            })
        };
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert!(
            !waiting.is_finished(),
            "the second check waits for the first"
        );
        drop(held);
        assert!(waiting.join().unwrap());
    }
}
