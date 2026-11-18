# PILOT READINESS V1

Baseline: HEAD 181b02b, workspace 0.2.1; 4 exercises, 9 cases. Previous evidence: repository report `reports/top3-final-2026-11-17.md`. Existing user edits are preserved. No publication or push.

Sequential stages: [A browser](browser-e2e.md), [B UX](ux-findings.md), [C learning](learning-tests.md), [D decision](GO-NO-GO.md). CPU engines, Visual Step, exercise v1 and isolated checker are reused. Final results below are filled only after execution.


## Final available evidence

- Real macOS GUI follow-up: **11 selected scenarios passed**, with actual native screenshots and fixtures; [illustrated report](desktop-gui.md). Existing profile, separate from automated tests and E01–E14; browser/iPad gates remain open.
- `cargo test --workspace --offline`: **499 passed, 0 failed, 2 ignored**, 53 suites including doctests. [Full log](evidence/pilot-workspace-release.log), [summary](test-results.json).
- Native components within the full run: 57 desktop, 21 tablet (including all 12 cards), 8 learning integration tests. All references satisfy their case suites; deliberately wrong programs cannot pass entire suites. No-op can legitimately pass zero-only cases.
- `cargo clippy --offline --workspace --all-targets -- -D warnings`, `cargo check --offline --target wasm32-unknown-unknown -p rustemu-web`, WASM Clippy, `cargo fmt --all --check`, `git diff --check`: passed; final logs in evidence.
- `trunk build --release --locked`: passed with frontend and assembler Worker. Static packaging does not verify actual Worker execution.
- `/usr/bin/python3 scripts/test_book_corpus.py`: 2 passed. NASM, GNU MIPS and compatibility_semantics executed within full suite. MARS not configured, hardware/speed ignored, not silently claimed verified.
- `/usr/bin/python3 scripts/package_pilot.py` creates unpublished `output/pilot/rustemu8086-pilot-readiness-v1.zip` with web, 12 exercises, reference solutions, roundtrip projects, reports, native screenshots and manifest. `/usr/bin/python3 scripts/test_pilot_package.py` validates payload; its observed results are appended after packaging.
- User sessions: NOT EXECUTED (0); physical iPad: MANUAL REQUIRED; E01-E14: BLOCKED; all five latency scenarios 0/10 measurements.

## Review and recovery

A: environment block recorded without circumventing it. B: optional first success, ISA grouping, inspectable cases, Edit code action and native bounds checks. C: 4 ->12 tasks / 6+6 ISA /34 cases, schema unchanged. D: scripts, immutable local package and manual GO gates. No engine rewrite/new ISA/deploy/push. Existing user changes in PROJECT_AUDIT.md, SPEACH.md, TODO.md and presentations preserved.

Before inviting users, complete real-browser and physical-device gates and rehearse rollback to a known-good immutable bundle. This candidate is not a claim of completed pilot participation. Decision: **GO CANDIDATE / MANUAL VALIDATION REQUIRED**, not external GO.

Static package validation: 3 passed (root resource paths/Worker loader, SHA256 manifest, 12-task inventory and references). Package contains only allowlisted app assets, practice projects, reference solutions and reports; no personal drive, credentials or user drafts. Archive hash is supplied as an external .zip.sha256 sidecar to avoid a self-referential digest.

## Authorized deployment follow-up

The user subsequently authorized publishing this candidate for physical-device validation. Commit `c89ee88` is now served by GitHub Pages; remote test job succeeded. See [deployment evidence and remaining network limits](deployment-2026-11-17.md) and [the real iPad checklist](../../docs/ipad-pilot-checklist.md). This changes the publication status only; browser/device GO gates remain open. Original statements about no deployment describe the earlier stage D checkpoint.
