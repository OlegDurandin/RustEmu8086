# C Learning tests

Focused reproduction commands (also covered in the executed full suite): `cargo test --offline -p rustemu-ui --test learning`, `cargo test --offline -p rustemu-desktop --lib pilot_first_success`, plus final workspace suite.

The catalog test requires exactly 12 unique IDs, 6/6 ISA balance, multiple cases per task, all references correct, all immediate-exit variants incorrect, deterministic repeated results, controlled loops and invalid assembly. Legacy roundtrip, malformed package, hostile budgets, output/memory mismatch, time limit, input exhaustion and private Session/personal drive tests remain. Frontend regression exercises First success and Cancel without source loss.

Final observed results are in README.md; no browser/iPad PASS is inferred. Native screenshot sources remain explicitly egui_kittest.

During expansion a negative-test assumption was corrected: a no-op is valid for some zero inputs. A wrong solution must fail the suite, not necessarily every individual case. Legitimate zero-case PASS is preserved; failed cases must explain Register/Memory/Output. This is a test-oracle correction, not an emulator fix. MIPS JAL task additionally asserts $ra and $sp.

Observed: learning integration 8 passed; frontend regression suite 57 desktop +20 tablet passed; extra pilot_catalog_cards test 1 passed across all 12 cards at 700x980. Final full workspace evidence is recorded in README. Superseded background full runs were interrupted after edits; only the final completed run is used for totals.

Final completed workspace: 499 passed, 0 failed, 2 ignored; current native tablet count is 21, including the new 12-card check. See evidence and test-results.json.
