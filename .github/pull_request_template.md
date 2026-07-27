## What this changes

<!-- And why. If it fixes a bug, what was the bug? -->

## Checklist

- [ ] `npm test` passes (vitest + cargo test)
- [ ] `npm run lint` passes
- [ ] `npm run contracts:check` — suites E and F still clean
- [ ] If behaviour and `contracts/` disagreed, the contract was updated too
- [ ] The StackVo checkout is untouched (`git -C ../stackvo status --porcelain`)
