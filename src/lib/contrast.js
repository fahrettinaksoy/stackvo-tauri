/**
 * Whether one colour can be read on another, and what to do when it cannot.
 *
 * This exists because of a measurement. The status palette — the green, orange,
 * red and blue that mean running, degraded, failed and idle — is used two ways:
 * as a **fill** (a dot, a chip, a progress bar) and as **text** (`text-success`
 * on a card). A colour can be perfectly good at the first and fail the second,
 * and the default palette does: `#4CAF50` on white is 2.77:1 and `#FB8C00` is
 * 2.37:1, against WCAG AA's 4.5 for body text. axe found ten of them on the
 * project page once the run stopped being scoped to `#app`.
 *
 * The fix is not to change the palette. A darker green would be a worse dot,
 * and the palettes are a setting somebody chose — `colorblind` is Okabe-Ito and
 * its values are the whole point of it. So the fill keeps the colour it was
 * given and the **text** gets a variant of it, darkened in a light theme and
 * lightened in a dark one, until it meets the threshold.
 *
 * ## Why the maths is here rather than a table of hand-picked colours
 *
 * Three palettes × four roles × two themes is twenty-four values to keep right
 * by hand, and the user can change the theme's surface colour under all of
 * them. Derived, it stays true; and being derived it can be tested, which a
 * table of hex codes somebody eyeballed cannot.
 *
 * WCAG 2.x contrast, exactly as the specification defines it — sRGB
 * linearisation, then `(L1 + 0.05) / (L2 + 0.05)`.
 */

/**
 * `#rgb`, `#rrggbb`, `r,g,b` — the third is the shape Vuetify's theme uses —
 * or an already-parsed triple.
 *
 * The triple is not a convenience. `readable` mixes colours and then asks for
 * the contrast of the mixture, and a `parse` that took strings only turned
 * every one of those questions into `null` — which compares false against
 * everything, so the loop ran a hundred times, improved nothing, and returned
 * the colour it was given. The failure was silent by construction: the function
 * is *allowed* to return its input, so nothing looked wrong until axe reported
 * the same 2.77:1 it had before.
 */
export function parse(color) {
  if (Array.isArray(color)) {
    return color.length === 3 && color.every((n) => Number.isFinite(n)) ? color : null;
  }
  if (typeof color !== 'string') return null;
  const text = color.trim();

  if (text.includes(',')) {
    const parts = text.split(',').map((p) => Number(p.trim()));
    return parts.length === 3 && parts.every((n) => Number.isFinite(n)) ? parts : null;
  }

  const hex = text.replace('#', '');
  if (hex.length === 3) {
    return [...hex].map((c) => parseInt(c + c, 16));
  }
  if (hex.length === 6) {
    return [0, 2, 4].map((i) => parseInt(hex.slice(i, i + 2), 16));
  }
  return null;
}

/**
 * @param {number[]} channels - `[r, g, b]`, unclamped and possibly fractional.
 * @returns {string}
 *
 * Typed as a list rather than as a three-tuple because every caller builds it
 * with `map`, which cannot produce a tuple. Destructuring in the parameter had
 * TypeScript infer `[any, any, any]`, and `npm run types:tsc` refused all three
 * call sites in this file — a shape none of them ever had.
 */
export function toHex([r, g, b]) {
  const clamp = (n) => Math.max(0, Math.min(255, Math.round(n)));
  return '#' + [r, g, b].map((n) => clamp(n).toString(16).padStart(2, '0')).join('');
}

/** Relative luminance, per WCAG 2.x. */
export function luminance(color) {
  const rgb = parse(color);
  if (!rgb) return null;
  const [r, g, b] = rgb.map((channel) => {
    const c = channel / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

/** The ratio, always ≥ 1, and symmetric in its arguments. */
export function contrast(a, b) {
  const la = luminance(a);
  const lb = luminance(b);
  if (la === null || lb === null) return null;
  return (Math.max(la, lb) + 0.05) / (Math.min(la, lb) + 0.05);
}

/** WCAG AA for body text. Large text is 3, and nothing here assumes large. */
export const AA_TEXT = 4.5;

/**
 * The same colour, moved until it can be read on `background`.
 *
 * Mixed toward black or toward white — whichever direction the background is
 * not — in one-percent steps. Steps rather than a solved equation because the
 * relationship between a mix ratio and the resulting contrast is not monotonic
 * in any form worth inverting, and a hundred iterations of arithmetic is free.
 *
 * The **hue is kept**: mixing toward black darkens green to a darker green, and
 * a status colour that changed hue on a light theme would be a status colour
 * that means something else. If even the endpoint does not reach the target —
 * possible on a mid-grey background, where neither black nor white gets there —
 * the closest attempt is returned rather than nothing, because a colour that is
 * 4.2:1 is better than the 2.4:1 it replaced.
 */
export function readable(color, background, target = AA_TEXT) {
  const rgb = parse(color);
  const bg = parse(background);
  if (!rgb || !bg) return typeof color === 'string' ? color : null;

  if (contrast(rgb, bg) >= target) return toHex(rgb);

  // Toward black on a light background, toward white on a dark one.
  const toward = luminance(bg) > 0.5 ? [0, 0, 0] : [255, 255, 255];

  let best = toHex(rgb);
  let bestRatio = contrast(rgb, bg);
  for (let step = 1; step <= 100; step += 1) {
    const ratio = step / 100;
    // Rounded to a real colour **before** it is measured. Measuring the
    // fractional mixture and returning its rounded form is a different colour
    // from the one that passed: `muted`'s blue came back at 4.4946 that way,
    // having been checked at 4.5001. Found by the test below, which is over
    // every palette rather than over one example — with one example it would
    // have passed.
    const mixed = toHex(rgb.map((channel, i) => channel + (toward[i] - channel) * ratio));
    const got = contrast(mixed, bg);
    if (got > bestRatio) {
      best = mixed;
      bestRatio = got;
    }
    if (got >= target) return mixed;
  }
  return best;
}
