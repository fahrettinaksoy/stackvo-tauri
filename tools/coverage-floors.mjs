/**
 * The coverage floors, in one place because two places drift.
 *
 * ## Why there is a floor now, when there deliberately was not one before
 *
 * The first version of this repository's coverage setup reported and never
 * failed, and the comment in `vitest.config.js` argued the case: a floor picked
 * before anyone has seen the number is either low enough to bless the gap or
 * high enough to fail on the first run, and both teach the same lesson — write
 * tests that move a percentage.
 *
 * That argument had an expiry date, and it has passed. The numbers have been
 * looked at across several rounds of work (61.60 → 63.12 → 63.34 → 64.34 on the
 * Rust side; 30.70 → 89.65 on the front end), so a floor set today is set from
 * evidence rather than from taste. Without one, the measurement answers "how
 * much is covered" and never "did that just get worse", which is the only
 * question a build can usefully ask.
 *
 * ## What a floor is for here
 *
 * It is a regression alarm, not a target. Nobody should write a test to move
 * these numbers; the number moves because tests were written for a reason. The
 * floors sit **below** today's measurement on purpose, and the gap is not
 * slack — it is the room the two facts below need:
 *
 *  1. **Coverage is platform-dependent on the Rust side.** These numbers come
 *     from macOS; CI measures on Ubuntu, where the `#[cfg(target_os = "macos")]`
 *     branches of `elevate.rs`, `certs.rs` and `apps.rs` compile out and their
 *     Linux counterparts compile in. The recorded history of this repository has
 *     the two within about a point of each other, and one point of drift must
 *     not turn a green build red.
 *  2. **A large, well-tested module still lowers the percentage while it is
 *     being written.** A floor with no headroom makes the first commit of a
 *     feature fail, which pushes people to write the test before the thing it
 *     tests is finished — the exact inversion this file is meant to prevent.
 *
 * Raising a floor after a real gain is a deliberate act. Lowering one is a
 * decision with a reason, and the reason belongs in the commit message.
 */

/**
 * Measured 2026-08-07 on macOS, at the commit that introduced this file.
 *
 * Kept next to the floors so the distance between them is visible: a floor
 * three points under a number that was last measured a year ago is not a floor,
 * it is a guess with a decimal point.
 */
export const measured = {
  rust: { lines: 64.05, functions: 59.23, regions: 64.69 },
  frontend: { lines: 89.65, statements: 89.65, branches: 76.91, functions: 53.34 },
};

export const floors = {
  /**
   * `cargo llvm-cov`'s line coverage for the whole crate.
   *
   * Lines rather than regions or functions: regions count each arm of every
   * `match` and swing with refactors that change nothing about what is tested,
   * and functions counts a one-line accessor the same as `run_operation`.
   */
  rust: { lines: 60 },

  /**
   * v8's numbers for `src/**`, as reported by `vitest run --coverage`.
   *
   * **`functions` is not floored, and that is not an oversight.** v8 counts
   * every arrow function as a function, and a Vue SFC compiles its template
   * into a render function full of them — inline handlers, slot props, `v-for`
   * bodies. The measured 53% does not mean half the front end's behaviour is
   * unexercised; it means half the closures a compiler emitted were never
   * called, and several of those cannot be called from jsdom at all. A floor on
   * a number nobody can act on teaches people to ignore a failing gate.
   *
   * `statements` and `lines` are the same figure under v8's instrumentation.
   * Both are listed so a future switch of provider does not silently drop one.
   */
  frontend: { lines: 85, statements: 85, branches: 72 },
};

export default floors;
