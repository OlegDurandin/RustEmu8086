# A Browser E2E

BLOCKED: environment. Prior Computer Use session explicitly prohibited further control of the local URL. No alternate port, driver or browser was used to circumvent that restriction. No new interactive browser run is claimed. Release build is checked separately; HTTP 200 and dist files do not prove Worker execution.

| ID | Scenario | Status |
|---|---|---|
| E01 | 8086 Hello Step Run | BLOCKED |
| E02 | INT16 PC keyboard | BLOCKED |
| E03 | MIPS Hello | BLOCKED |
| E04 | MIPS sum Run I/O | BLOCKED |
| E05 | Step Back Breakpoint both ISA | BLOCKED |
| E06 | Switch ISA and preserve edits | BLOCKED |
| E07 | remu clean profile roundtrip | BLOCKED |
| E08 | Share and long link fallback | BLOCKED |
| E09 | Four reference exercises in browser | BLOCKED |
| E10 | Four wrong solutions in browser | BLOCKED |
| E11 | Stop Cancel loop | BLOCKED |
| E12 | Assembly error recovery | BLOCKED |
| E13 | Save source versus project | BLOCKED |
| E14 | Corrupt remu preservation | BLOCKED |

[Machine readable results](browser-e2e-results.json). Lower-layer evidence: ux_* and learning_card harness, share roundtrips, compatibility_semantics and learning tests; these are NOT browser E2E.

Resume in an authorized browser: build with `cd crates/rustemu-web && trunk build --release --locked`, serve through an approved local server and execute PILOT-TEST-SCRIPT.md. Record browser/version/OS/build SHA, actual actions, expected and observed values, logs and screenshots. Explicitly observe assembler Worker completion on Assemble and Check, clean-profile import, keyboard and downloads. Never change BLOCKED to PASS based on native tests.
