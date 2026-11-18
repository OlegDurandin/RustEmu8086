# D Closed pilot script for 5-10 volunteers

NOT EXECUTED. No users were contacted. Invite only after manual validation gates below. Roles: 2-4 beginners, 2-4 students, 1-2 teachers/experienced users (total 5-10). No account/backend needed.

Record participant code (no personal data), role, device/OS/browser, build SHA, start/end, assistance, actual result, screenshot/log and issue. Stop immediately on lost work, false learning PASS or unrecoverable hang. Keep/export a copy of work before testing.

1. E01/E03: First success -> choose ISA Hello -> Step -> Run; obtain output within target 2 minutes without assistance.
2. E02: 8086 keyboard program -> waiting -> PC key -> continue without Reset. E04: sum.s -> 5 -> Send -> sum 15.
3. E05: Step/undo and breakpoint in each ISA; explain Last versus Next and changed register.
4. E06: edit both ISA sources and settings; switch back and verify exact edits.
5. E12: deliberately use MOV AL,300 / bad MIPS address or register; read line diagnostic, fix and assemble again.
6. E09/E10: open each original task, run reference and deliberately wrong solution; check expected/actual, return and edit. Then sample the new track.
7. E11: loop -> Stop; loop exercise -> Cancel; record 10 latency samples per scenario using stop-latency.md.
8. E07/E08/E13: export examples/projects/8086-include.remu (X!) and mips-input.remu (40 then 42), import in a clean profile; compare include, dialect, MARS and input. Compare source-only save, local draft and full project. Test short link and long-link file warning.
9. E14: import truncated JSON and unsupported version while unsaved edits exist; expect error with exact original text retained.
10. On real iPad execute every row of ipad-checklist.md, including IME/Files/zoom.

Results form per action: scenario ___ status PASS/FAIL ___ expected ___ observed ___ elapsed ___ help ___ evidence ___ issue ___ . First-run target <=2 min; 0 lost edits; no false PASS; Stop goal <=100 ms is NOT measured yet.

Recovery: stop testing, export work if possible, record error/build, reload only after preserving source. Keep previous bundle separately. Rollback swaps the complete static web directory, never overwrites user projects; do not clear browser storage.
