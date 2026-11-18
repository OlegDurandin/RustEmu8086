# B UX findings and first success

Existing state model, byte halves, ISA-specific tools, input, Last/Next, save/export and zoom viewport are retained.

Added optional First success in existing Examples menu: select ISA Hello, Run, Step Back, open Learn. No permanent window or mandatory wizard. Learn is grouped by ISA; topic is part of each title. Public case inputs and initial registers are inspectable in the exercise card without changing remu v1.

Physical iPad remains MANUAL REQUIRED. Native layout tests and new onboarding regression are recorded in final README. No new observed browser defect can be inferred from the blocked browser. Earlier card clipping regression remains active.

Edit code is now a 44-point action near the top of the existing card, closing it before editing/IME; Return to code remains available. Card height is bounded by the viewport. Regression verifies closing preserves source after cancelling a check. Physical keyboard/IME interaction still needs the device checklist.

All 12 initial cards passed native bounds and 44-point Edit/Check checks at 700x980 and returned to the unchanged editor. All 12 PNGs reviewed; source is egui_kittest, not a browser or iPad. [Screenshots](screenshots/). Opening hints/public cases and results with the system keyboard still needs physical validation.

D packaging finding: default Trunk assets use root URLs; serving the archive root with /web/ would break loading. START-HERE now serves web as the HTTP root. Static package regression verifies instructions, root asset paths, Worker loader, manifest and catalog inventory. This is packaging validation, not E2E.
