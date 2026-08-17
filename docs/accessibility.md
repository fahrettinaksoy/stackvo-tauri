# Accessibility statement

**StackVo Desktop** — statement prepared 16 August 2026, against the code in
this repository at that date.

This is a statement about a desktop application, written to the shape EN 301 549
asks for (§9, which adopts WCAG 2.2 Level AA for non-web software). It is
published in the repository rather than on a website because that is where the
software is, and because every number in it is reproducible from a checkout with
one command.

---

## 1. Conformance status

**Partially conformant with WCAG 2.2 Level AA.** Partially, and the word is
doing real work: the automated criteria are met on every screen and the
criteria a machine cannot decide have not been audited by a person or with a
screen reader. §4 says exactly which those are.

Nothing here is a claim about a certification. No third party has audited this
application, and this statement is a self-assessment.

## 2. What was measured, and how

`axe-core` 4.x, run through Playwright against Chromium, over the **rendered
page in a real engine** — layout done, colours computed, overlays where they
actually end up in the document.

```sh
npm run test:e2e          # tests/e2e/a11y.e2e.js
```

**Result: zero violations, at every severity, on all nine routes.**

| Route | Violations |
| --- | --- |
| Dashboard `/` | 0 |
| Projects `/projects` | 0 |
| Project detail `/projects/:name` | 0 |
| Market `/market` | 0 |
| Logs `/logs` | 0 |
| Dumps `/dumps` | 0 |
| Mail `/mail` | 0 |
| Settings `/settings` | 0 |
| About `/about` | 0 |

The list is the router's own, and `tests/accessibility-claims.spec.js` fails if
this table and the routes come apart — a statement about "the application" that
quietly measured four of nine screens is the failure mode this table exists to
prevent.

Two more checks run in the same engine, in `tests/e2e/shell.e2e.js`: that every
visible control has a size a pointer can hit, and that the keyboard reaches
something and that what it reaches can be seen. Beside those, 800-odd unit tests
include an axe pass over individual components (`tests/a11y-axe.spec.js`) and a
source scan for icon-only buttons with no accessible name (`tests/a11y.spec.js`).

### What the measurement used to miss

Recorded because a conformance statement is worth exactly as much as the
measurement under it, and this one was weaker than it looked until August 2026.

* The axe run was **scoped to `#app`**. Vuetify's overlay container is a sibling
  of `#app`, not a child — so every tooltip, menu, dialog and side sheet in the
  application was outside the measurement, along with the two rules that only
  exist against `<html>`.
* Worse, those overlays were **covering the page**. A closed overlay kept a
  full-viewport box at `z-index: 2000`, so axe could not determine a background
  for anything underneath and skipped the contrast rule almost entirely. The run
  reported "zero serious violations" because the page was hidden from it.

Both are fixed. What the corrected run then found — and what §5 records as
fixed — was seventeen real failures across four screens, none of which any
previous run could have reported.

## 3. Accessibility features

These are settings, in **Settings → Appearance**, and they persist:

* **High contrast**, which raises the emphasis and border opacities the whole
  interface is drawn through.
* **Status palette**, including an **Okabe-Ito** option: the colours that mean
  running, degraded, failed and idle, chosen to stay distinguishable under every
  common form of colour-vision deficiency. Status is never carried by hue alone
  — every coloured dot has a label or an icon beside it.
* **Interface scale**: root font size, so the whole type scale grows with it
  rather than one label at a time.
* **Reduced motion**, honoured for transitions and animated meters.
* **Density**, three steps, applied globally rather than per component.
* **Right-to-left**, as a layout preference rather than a property of the
  chosen language.
* **Theme**: light, dark, or the desktop's own setting, followed live.

Beyond the settings: text is real text and never an image of text; every icon
button carries an accessible name (there is a test that fails the build
otherwise); the terminal and log views can be read at any interface scale; and
the application is fully operable from the keyboard, with a visible focus
indicator on what the keyboard reaches.

## 4. Known limitations

Stated plainly, because a statement without them is a marketing page.

* **No screen-reader audit.** Nothing here has been driven with VoiceOver, NVDA
  or Orca by a person. Automated tooling decides roughly a third of WCAG's
  success criteria; the rest — whether a label *makes sense*, whether an error
  message says what to do, whether the reading order matches the visual one —
  needs a human, and no human has done it.
* **No audit of the native window.** The measurement drives the front end in a
  browser engine. The window chrome, the menu bar and the tray menu are the
  operating system's, reached through Tauri, and are not covered.
  `tauri-driver`, which would cover them, does not run on macOS — Tauri's own
  documentation says so — and this application is developed on macOS. That is
  item #12 in `docs/durum.md`.
* **Dialogs and drawers are measured as they load.** The axe pass opens each
  route and measures what is on it. A dialog somebody opens by clicking is in
  the overlay container and therefore in scope, but only if something opened it
  during the run.
* **No conformance claim for third-party content.** The Market lists packages
  whose descriptions come from a catalogue this application does not write.
* **Language attributes.** The interface language is announced on the document.
  A view mixing two languages — a Turkish interface showing an English log line
  — does not mark the change per passage.

## 5. What was fixed to make this statement true

Every one of these was found by the corrected measurement in §2, and each is a
WCAG failure rather than a piece of advice:

| What | Where it was | Criterion |
| --- | --- | --- |
| Closed tooltips exposed as unnamed `role="tooltip"` nodes | every page that has built one | 4.1.2 Name, Role, Value |
| Three `<nav>` landmarks, none of them named | the whole application shell | 1.3.1 Info and Relationships |
| No `<h1>` on any page | every page | 1.3.1, 2.4.6 Headings and Labels |
| A table column header with no text | Projects | 1.3.1 |
| Page subtitle at 3.62:1 on the primary bar | every page | 1.4.3 Contrast (Minimum) |
| Secondary text at 2.67:1 (`text-grey` on white) | Dashboard, Projects | 1.4.3 |
| Field labels at 4.25:1 and 3.97:1 | every form in the application | 1.4.3 |
| Tile labels at 4.49:1 and footers at 3.42:1 | project detail | 1.4.3 |
| Status colours as text: green 2.77:1, orange 2.37:1 | project detail, and every pane using them | 1.4.3 |

The last one is worth a note, because the fix is not "use a different palette".
A status colour has two jobs — a fill (a dot, a chip) and text — and the palette
is chosen for the first. `src/lib/contrast.js` derives a **text variant** of each
status colour against the theme's own surface, moved only far enough to meet
4.5:1 and with the hue kept, so the dot stays the colour somebody picked and the
sentence beside it can be read. It is derived rather than hand-picked because
three palettes × four roles × two themes is twenty-four values, and the user can
change the surface under all of them.

## 6. Feedback

If something here is wrong, or if you are blocked by something this statement
does not mention, open an issue in this repository. That is the only channel,
and saying so is better than naming one nobody watches.
