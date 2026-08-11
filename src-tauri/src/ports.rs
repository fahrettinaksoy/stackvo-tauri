//! Deciding which host port an instance publishes on.
//!
//! Faz 2 of `docs/servis-market-mimarisi.md`, and the half of multiple versions
//! that is not about names. Two MySQL instances cannot both publish 3306, and
//! nothing in the tree today decides that for anybody: the number is written
//! into a template, by hand, once per service.
//!
//! Measured before writing this, because the state it replaces is worse than it
//! looks:
//!
//! * There are **two** key families, `HOST_PORT_<ID>` (17 keys) and
//!   `SERVICE_<ID>_HOST_PORT` (14). `contracts/env.schema.json` records the
//!   overlap under `servicePattern` and says v1 should standardise on one.
//! * **Thirteen services** read the first family, and not one of those keys has
//!   a default in `config::EMBEDDED`. The Services sheet lists the keys a
//!   service has; with no key there is no row, so MySQL's published port could
//!   not be changed from the app at all.
//! * The hand-assigned numbers already collide. The Mongo Express template
//!   defaults to `8081`, which is phpMyAdmin's. What keeps them apart today is
//!   `SERVICE_MONGO_EXPRESS_HOST_PORT=8083` in a different file — so the
//!   correct answer is not in the template, and the template is still wrong.
//!
//! ## Four inputs, and the fourth is the one that cannot be skipped
//!
//! 1. **The manifest's preference.** Free means taken, so a single-instance
//!    user keeps the number they have today and nothing appears to change.
//! 2. **What the table has spoken for**, including instances that are switched
//!    off. Reusing a disabled instance's port turns "switch it back on" into a
//!    bind failure much later, with nothing on screen connecting the two.
//! 3. **A deterministic stride** — `preferred + 10·n` — rather than "next free".
//!    A user memorises the port their second MySQL got; it must survive a
//!    reinstall, and "next free" depends on what else happened to be installed
//!    that day.
//! 4. **The kernel.** `instances.json` knows nothing about the Postgres someone
//!    installed with Homebrew. Only a `bind` finds that, and finding it here is
//!    the difference between an explanation now and `docker compose up` failing
//!    with a number and no name.
//!
//! Allocation happens once and is written down. It is deliberately not
//! recomputed per render: a connection string that changes because an unrelated
//! service was installed is a string somebody had already pasted somewhere.

use crate::error::{Code, Error, Result};
use std::collections::BTreeSet;
use std::net::TcpListener;

/// How far apart successive instances of one service land.
///
/// Ten, so the numbers stay readable — 3306, 3316, 3326 — and so a service that
/// publishes several ports does not walk into its own next instance.
pub const STRIDE: u16 = 10;

/// How many strides to try before giving up.
///
/// Sixteen is far past any real arrangement; it exists so a machine with a
/// pathological port map produces an error instead of a loop.
const ATTEMPTS: u16 = 16;

/// Is anything already listening here?
///
/// Binding rather than connecting, because the question is "can this container
/// publish here", and a socket in TIME_WAIT answers a connect but refuses a
/// bind. `SO_REUSEADDR` is deliberately NOT set: it would let this succeed
/// exactly where Docker would later fail.
///
/// Both stacks are checked. A listener on `0.0.0.0` does not stop a bind to
/// `127.0.0.1` on every platform, and Docker publishes to both.
pub fn is_free(port: u16) -> bool {
    TcpListener::bind(("127.0.0.1", port)).is_ok() && TcpListener::bind(("0.0.0.0", port)).is_ok()
}

/// What an allocation may not take, beyond what the instance table holds.
#[derive(Debug, Clone, Default)]
pub struct Claims {
    /// Ports already promised in this same round. An allocator called once per
    /// port of one manifest must not hand the same number to two of them.
    pub pending: BTreeSet<u16>,
}

/// Choose a host port for one manifest entry.
///
/// `reserved` is [`crate::instances::Table::reserved_ports`]. `probe` answers
/// "is this free on the machine" — [`is_free`] in production, a closure in
/// tests, because a test that binds real sockets fails on whichever CI machine
/// happens to run something.
pub fn allocate(
    preferred: u16,
    reserved: &BTreeSet<u16>,
    claims: &mut Claims,
    probe: &dyn Fn(u16) -> bool,
) -> Result<u16> {
    if preferred == 0 {
        return Err(Error::new(
            Code::InvalidInput,
            "a manifest port of 0 asks the kernel to choose, which cannot be written down",
        ));
    }

    for n in 0..ATTEMPTS {
        let Some(candidate) = preferred.checked_add(STRIDE.saturating_mul(n)) else {
            break;
        };
        if reserved.contains(&candidate) || claims.pending.contains(&candidate) {
            continue;
        }
        if !probe(candidate) {
            continue;
        }
        claims.pending.insert(candidate);
        return Ok(candidate);
    }

    Err(Error::new(
        Code::Conflict,
        format!(
            "no free host port near {preferred} — tried {preferred} and every \
             {STRIDE}th port up to {}",
            preferred.saturating_add(STRIDE.saturating_mul(ATTEMPTS - 1))
        ),
    )
    .with_hint(crate::hints::PORT_RANGE_EXHAUSTED))
}

/// Keep a port that is already ours, or move it if the machine has taken it.
///
/// The migration path. An instance adopted from `.env` should keep publishing
/// the number a user's tooling already has written down, and that number is
/// almost always still free — it is the port the service was using. But
/// "almost always" is not "always": a workspace restored onto a machine that
/// runs its own MySQL would otherwise produce an instance that can never start,
/// and the honest answer is a different port and a note, not a failure.
pub fn keep_or_move(
    current: u16,
    preferred: u16,
    reserved: &BTreeSet<u16>,
    claims: &mut Claims,
    probe: &dyn Fn(u16) -> bool,
) -> Result<u16> {
    if current != 0
        && !reserved.contains(&current)
        && !claims.pending.contains(&current)
        && probe(current)
    {
        claims.pending.insert(current);
        return Ok(current);
    }
    allocate(preferred, reserved, claims, probe)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn everything_free(_: u16) -> bool {
        true
    }

    fn none() -> BTreeSet<u16> {
        BTreeSet::new()
    }

    #[test]
    fn the_first_instance_gets_the_number_it_has_today() {
        let mut claims = Claims::default();
        assert_eq!(
            allocate(3306, &none(), &mut claims, &everything_free).unwrap(),
            3306
        );
    }

    /// The second lands somewhere a person can remember, and lands there again
    /// on the next machine.
    #[test]
    fn the_second_instance_strides_rather_than_taking_the_next_free_number() {
        let reserved = BTreeSet::from([3306]);
        let mut claims = Claims::default();
        assert_eq!(
            allocate(3306, &reserved, &mut claims, &everything_free).unwrap(),
            3316
        );
        // Deterministic: the same inputs give the same answer, which is why a
        // reinstall does not renumber anything.
        let mut again = Claims::default();
        assert_eq!(
            allocate(3306, &reserved, &mut again, &everything_free).unwrap(),
            3316
        );
    }

    /// A switched-off instance keeps its number.
    ///
    /// `reserved_ports` includes disabled instances on purpose, and this is the
    /// test that says why: handing 3306 away while MySQL 8.0 is merely off
    /// turns switching it back on into a bind error nothing on screen explains.
    #[test]
    fn a_reserved_port_is_skipped_even_when_nothing_is_listening() {
        let reserved = BTreeSet::from([3306, 3316]);
        let mut claims = Claims::default();
        assert_eq!(
            allocate(3306, &reserved, &mut claims, &everything_free).unwrap(),
            3326
        );
    }

    /// The input the table cannot have: somebody else's Postgres.
    #[test]
    fn a_port_the_machine_already_uses_is_skipped() {
        let busy = |p: u16| p != 5432;
        let mut claims = Claims::default();
        assert_eq!(allocate(5432, &none(), &mut claims, &busy).unwrap(), 5442);
    }

    /// One manifest with two ports must not be given one number twice.
    #[test]
    fn two_ports_in_the_same_round_do_not_collide() {
        let mut claims = Claims::default();
        let a = allocate(8081, &none(), &mut claims, &everything_free).unwrap();
        let b = allocate(8081, &none(), &mut claims, &everything_free).unwrap();
        assert_eq!((a, b), (8081, 8091));
    }

    #[test]
    fn an_exhausted_range_is_an_error_and_not_a_loop() {
        let mut claims = Claims::default();
        let err = allocate(3306, &none(), &mut claims, &|_| false).unwrap_err();
        assert_eq!(err.code, Code::Conflict);
        assert!(err.message.contains("3306"), "{}", err.message);
    }

    /// A port near the top of the range must not wrap around to a low one.
    #[test]
    fn a_high_preference_stops_rather_than_overflowing() {
        let mut claims = Claims::default();
        let err = allocate(65535, &none(), &mut claims, &|_| false).unwrap_err();
        assert_eq!(err.code, Code::Conflict);
    }

    #[test]
    fn zero_is_refused_rather_than_asked_of_the_kernel() {
        let mut claims = Claims::default();
        assert!(allocate(0, &none(), &mut claims, &everything_free).is_err());
    }

    /// Migration keeps the number a user's tooling already has.
    #[test]
    fn an_adopted_port_is_kept_when_it_is_still_free() {
        let mut claims = Claims::default();
        assert_eq!(
            keep_or_move(3307, 3306, &none(), &mut claims, &everything_free).unwrap(),
            3307
        );
    }

    /// …and moves rather than failing when it is not.
    #[test]
    fn an_adopted_port_moves_when_the_machine_has_taken_it() {
        let busy = |p: u16| p != 3307;
        let mut claims = Claims::default();
        assert_eq!(
            keep_or_move(3307, 3306, &none(), &mut claims, &busy).unwrap(),
            3306
        );
    }

    /// A `.env` with no port for a service migrates to the manifest's number.
    #[test]
    fn an_instance_with_no_current_port_falls_back_to_the_preference() {
        let mut claims = Claims::default();
        assert_eq!(
            keep_or_move(0, 6379, &none(), &mut claims, &everything_free).unwrap(),
            6379
        );
    }

    /// A port somebody is listening on reads as taken.
    ///
    /// The deterministic half: the listener is held for the length of the
    /// assertion, so nothing else on the machine can change the answer.
    #[test]
    fn a_port_being_listened_on_is_not_free() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let taken = listener.local_addr().unwrap().port();

        assert!(!is_free(taken));
        // Twice, because a probe that consumed the socket would answer
        // differently the second time.
        assert!(!is_free(taken));
        drop(listener);
    }

    /// The probe binds rather than connects, so it must give the port back.
    ///
    /// Written as a bounded search rather than one attempt, and the first
    /// version was the one attempt: it picked an ephemeral port, released it,
    /// and asserted it was still free — which is a race against six hundred
    /// other tests running in parallel, and it lost. What is being tested is a
    /// property of `is_free`, not of the machine's port map, so the test is
    /// allowed to look for a port nobody is fighting it for; it fails only if
    /// `is_free` says "free" and the bind that follows disagrees.
    #[test]
    fn probing_a_port_does_not_keep_it() {
        for _ in 0..32 {
            let candidate = {
                let l = TcpListener::bind(("127.0.0.1", 0)).unwrap();
                let p = l.local_addr().unwrap().port();
                drop(l);
                p
            };
            if !is_free(candidate) {
                continue; // Somebody took it between the two calls. Try another.
            }
            // The probe said free and then let go of it, so this must work.
            assert!(
                TcpListener::bind(("127.0.0.1", candidate)).is_ok(),
                "is_free({candidate}) answered true but left the port bound"
            );
            return;
        }
        panic!("no ephemeral port stayed free long enough to test with");
    }
}
