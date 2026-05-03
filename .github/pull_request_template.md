## Summary

<!-- One or two sentences describing the change. Why does this exist? -->

## Test plan

<!-- How did you verify this works? If you tested against a real Garmin
device, list the model + firmware. Headless CLI test commands are great. -->

- [ ] `./scripts/check.sh --full` passed locally (rustfmt, clippy, build,
      tests, cargo audit, cargo deny)
- [ ] Tested on hardware (model: …, firmware: …)
- [ ] Updated docs / README if user-facing
- [ ] Updated `Cargo.lock` (if dependencies changed)
- [ ] No new network-capable deps introduced (or: discussed first per
      [CONTRIBUTING.md](../CONTRIBUTING.md))

## Related issues

<!-- Closes #N, refs #M, etc. -->
