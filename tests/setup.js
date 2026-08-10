/**
 * Browser APIs jsdom does not implement, stubbed so components can mount.
 *
 * These are not conveniences that hide bugs — they are the opposite. Without
 * them, Vuetify's `VProgressCircular` and `VOverlay` throw during `setup()`,
 * which means the pages that use them cannot be mounted at all, which is
 * exactly how `src/views/` stayed at 0% coverage while nine files and 9,490
 * lines went unverified.
 *
 * Each stub is inert on purpose. Reporting a plausible-looking size would let a
 * test assert on a layout jsdom never performed; observing nothing and
 * measuring zero is honestly what a headless DOM knows. Anything that genuinely
 * depends on layout needs a real browser — see the note about `tauri-driver` in
 * `views-render.spec.js`.
 */

if (!globalThis.ResizeObserver) {
  globalThis.ResizeObserver = class ResizeObserver {
    observe() {}
    unobserve() {}
    disconnect() {}
  };
}

if (!globalThis.IntersectionObserver) {
  globalThis.IntersectionObserver = class IntersectionObserver {
    constructor(callback) {
      this.callback = callback;
    }
    observe() {}
    unobserve() {}
    disconnect() {}
    takeRecords() {
      return [];
    }
  };
}

// `VOverlay` — every dialog, menu and tooltip — reads `visualViewport` while
// positioning itself, and jsdom does not declare the global at all, so the
// reference *throws* rather than being undefined. A component that opens a
// dialog therefore could not be tested, which is how the About window's licence
// notice nearly shipped with no test behind it.
//
// Declared as `undefined` rather than faked: Vuetify falls back to `window`
// when it is absent, and a stub reporting a plausible viewport would let a test
// assert on a layout that never happened.
if (!('visualViewport' in globalThis)) {
  globalThis.visualViewport = undefined;
}

// Vuetify's display composable reads this on mount; jsdom has no media engine.
if (!globalThis.matchMedia) {
  globalThis.matchMedia = (query) => ({
    matches: false,
    media: query,
    onchange: null,
    addListener() {},
    removeListener() {},
    addEventListener() {},
    removeEventListener() {},
    dispatchEvent: () => false,
  });
}

// `VVirtualScroll` and the data tables ask for one; jsdom returns nothing from
// `getBoundingClientRect`, so a zero-size rect is the truthful answer.
if (!globalThis.requestAnimationFrame) {
  globalThis.requestAnimationFrame = (cb) => setTimeout(() => cb(Date.now()), 0);
  globalThis.cancelAnimationFrame = (id) => clearTimeout(id);
}

// `VSparkline` measures its own path to animate the stroke. jsdom implements
// SVG elements but none of their geometry, so the call is missing rather than
// wrong — and an animation length is not something any assertion here depends
// on.
if (typeof SVGElement !== 'undefined' && !SVGElement.prototype.getTotalLength) {
  SVGElement.prototype.getTotalLength = () => 0;
  SVGElement.prototype.getPointAtLength = () => ({ x: 0, y: 0 });
}
