//! Keeping one operation per subject actually one operation.
//!
//! The front end tracks a busy flag per project, but that is one view's idea of
//! what is happening. The tray menu, a second view and a keyboard shortcut all
//! reach the same commands, and none of them can see the others' flag. Two
//! `docker compose up` runs against the same project, or a stop racing a
//! restart, produce failures that look like Docker being flaky.
//!
//! Two different problems, two different answers:
//!
//! * A user-initiated operation on a subject already busy is a *mistake* — a
//!   double click, a stale button. It should fail immediately and say so, not
//!   queue up and surprise someone a minute later. That is this module.
//! * Generation is an internal step of many operations and its files are
//!   shared. Two builds must not both write `docker-compose.projects.yml`, but
//!   failing one because the other happened to regenerate at that instant would
//!   be wrong. Those queue instead — see `AppState::generate_lock`.

use crate::error::{Code, Error, Result};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

/// Subjects with an operation in progress.
#[derive(Clone, Default)]
pub struct Registry {
    busy: Arc<Mutex<HashSet<String>>>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Claim `key`, or fail if someone already holds it.
    ///
    /// The key is scoped by kind (`project:shop`, `service:redis`, `stack`) so
    /// a project and a service that happen to share a name do not block each
    /// other, and so the whole-stack commands serialise against each other.
    pub fn acquire(&self, key: impl Into<String>) -> Result<Guard> {
        let key = key.into();
        let mut busy = self
            .busy
            .lock()
            .map_err(|_| Error::new(Code::IoError, "in-flight lock poisoned"))?;

        if !busy.insert(key.clone()) {
            return Err(Error::new(
                Code::Conflict,
                format!("an operation on {key} is already running"),
            )
            .with_hint(crate::hints::WAIT_FOR_OPERATION));
        }

        Ok(Guard {
            key,
            busy: Arc::clone(&self.busy),
        })
    }

    /// Whether a subject is currently claimed. Only used by tests — the answer
    /// is stale the moment it is returned, so no caller should branch on it.
    #[cfg(test)]
    fn is_busy(&self, key: &str) -> bool {
        self.busy.lock().map(|b| b.contains(key)).unwrap_or(false)
    }
}

/// Releases on drop, including on an early `?` return or a panic. That matters
/// more than it looks: a command that fails halfway and leaves its subject
/// marked busy would lock the user out of that project until a restart.
#[derive(Debug)]
pub struct Guard {
    key: String,
    busy: Arc<Mutex<HashSet<String>>>,
}

impl Drop for Guard {
    fn drop(&mut self) {
        if let Ok(mut busy) = self.busy.lock() {
            busy.remove(&self.key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_claim_on_the_same_subject_is_refused() {
        let reg = Registry::new();
        let _first = reg.acquire("project:shop").unwrap();

        let second = reg.acquire("project:shop");
        assert!(second.is_err());
        assert_eq!(second.unwrap_err().code, Code::Conflict);
    }

    #[test]
    fn different_subjects_do_not_block_each_other() {
        let reg = Registry::new();
        let _a = reg.acquire("project:shop").unwrap();
        let _b = reg.acquire("project:blog").unwrap();
        // Same name, different kind — these are different things.
        let _c = reg.acquire("service:shop").unwrap();
    }

    #[test]
    fn the_claim_is_released_when_the_guard_drops() {
        let reg = Registry::new();
        {
            let _g = reg.acquire("project:shop").unwrap();
            assert!(reg.is_busy("project:shop"));
        }
        assert!(!reg.is_busy("project:shop"));
        reg.acquire("project:shop").expect("released, so claimable");
    }

    /// The failure this guards: an early return must not leave the subject
    /// marked busy forever.
    #[test]
    fn an_error_path_still_releases_the_claim() {
        let reg = Registry::new();

        fn fails(reg: &Registry) -> Result<()> {
            let _guard = reg.acquire("project:shop")?;
            Err(Error::new(Code::BuildFailed, "boom"))
        }

        assert!(fails(&reg).is_err());
        assert!(
            !reg.is_busy("project:shop"),
            "the guard dropped on the way out"
        );
    }

    #[test]
    fn a_panicking_operation_still_releases_the_claim() {
        let reg = Registry::new();
        let cloned = reg.clone();

        let result = std::panic::catch_unwind(move || {
            let _guard = cloned.acquire("project:shop").unwrap();
            panic!("operation exploded");
        });

        assert!(result.is_err());
        assert!(!reg.is_busy("project:shop"));
    }

    #[test]
    fn many_threads_racing_one_subject_yield_exactly_one_winner() {
        let reg = Registry::new();
        let winners = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let barrier = Arc::new(std::sync::Barrier::new(16));

        let handles: Vec<_> = (0..16)
            .map(|_| {
                let reg = reg.clone();
                let winners = Arc::clone(&winners);
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    if let Ok(guard) = reg.acquire("project:shop") {
                        winners.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        // Hold it long enough that the losers are genuinely
                        // contending rather than arriving after the release.
                        std::thread::sleep(std::time::Duration::from_millis(40));
                        drop(guard);
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(
            winners.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "exactly one thread may hold a subject"
        );
    }
}
