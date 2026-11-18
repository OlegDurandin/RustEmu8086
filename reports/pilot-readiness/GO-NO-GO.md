# D Decision

GO CANDIDATE / MANUAL VALIDATION REQUIRED. Not GO for distribution to external participants yet. No real users tested and no physical iPad tested. E01-E14 and Stop timing remain blocked/unmeasured; unknown browser P0 cannot be declared absent.

Required gates: authorized browser operator completes all E01-E14 with build/version/evidence, confirms real Worker, no false PASS, no lost edits and responsive Cancel; physical Safari owner completes device checklist; 10 latency samples for each of 5 scenarios; review any P0 and regressions; then sign off immutable bundle SHA.

Confirmed loss of work, inability to continue input, false PASS or persistent browser hang => NO-GO and a minimal regression-backed fix. No such new observed production defect is claimed from unavailable environments.

Available native/semantic/build evidence is summarized in README.md and learning-tests.md. Prior engines and visual debugging are preserved; exercise count is 12, not 12 new. No deploy, push or user invitations performed.

Final available evidence: 499 Rust tests passed, 0 failed, 2 ignored; native/WASM Clippy and release passed; 12 tasks/34 cases; all 12 narrow native cards inspected; 3 static package checks passed. These do not unlock the real-browser or physical-device gates.

Follow-up after user authorization: 11 selected real macOS GUI scenarios passed (execution and input for both ISAs, step/back, wrong/correct checker results, malformed project, export/reopen, Stop, assembly-error recovery, unsaved-change Cancel). See [illustrated observations](desktop-gui.md). Existing profile, not complete E01–E14; Stop is functionally observed but not timed. Decision remains unchanged.

Open validation gates (not invented confirmed bugs): P0 E01-E14/actual Worker, input recovery, no false PASS and preservation in clean profile; P1 physical IME/Files/zoom, measured Stop latency and a rehearsed immutable rollback. Owner: pilot coordinator/device owner. Close only with raw observations and build fingerprint.

## Authorized deployment follow-up

The user subsequently authorized publishing this candidate for physical-device validation. Commit `c89ee88` is now served by GitHub Pages; remote test job succeeded. See [deployment evidence and remaining network limits](deployment-2026-11-17.md) and [the real iPad checklist](../../docs/ipad-pilot-checklist.md). This changes the publication status only; browser/device GO gates remain open. Original statements about no deployment describe the earlier stage D checkpoint.
