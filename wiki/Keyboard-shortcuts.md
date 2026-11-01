# Keyboard shortcuts

**English** · [Русский](Горячие-клавиши)

⌘ is Command, ⌃ is Control, ⇧ is Shift. The same keys work in the browser, except ⌘N, ⌘W and ⌘T: the browser
does not pass them to the page, so use the menu.

## Files and building

| Keys | Action |
|---|---|
| ⌘N | new file (template window) |
| ⌘O | open |
| ⌘S / ⇧⌘S | save / save as |
| **F9** or ⌘E | assemble and load into the emulator (*emulate*) |
| ⌘B | assemble to a file (*compile*) |

## Running

| Keys | Action |
|---|---|
| **F5** or ⌘R | Run / Stop |
| **F8** | step (Single step) |
| **F10** | step over a macro, CALL, INT (Step over) |
| **⇧F11** | leave the procedure or macro (Step out) |
| **⇧F8** or F6 | step back (Step back) |
| **F4** | run to the line with the cursor (Run to cursor) |
| ⌘F8 | breakpoint on the line |
| F7 | show the current instruction |
| ⌃F8 | run to the instruction selected in the disassembly (run until) |
| ⌃⇧F7 | move CS:IP to the selected instruction (set CS:IP) |
| F11 | debugger console and log |

## Editor

| Keys | Action |
|---|---|
| F1 or ⌘-click | reference article for the word under the cursor |
| ⌘F, ⌘H, ⌃H | find and replace |
| ⌘G / F3 | find next |
| ⇧⌘G / ⇧F3 | find previous |
| ⌘L or ⌃G | go to line |
| ⌘/ | comment / uncomment |
| ⌃Q / ⌃W | comment / uncomment (Emu8086) |
| ⌃L / ⌃U | lower / upper case |
| ⌃F2 or ⌘F2 | set / clear a bookmark |
| F2 / ⇧F2 | next / previous bookmark |

## Differences from Emu8086

| Action | Emu8086 | RustEmu8086 |
|---|---|---|
| run / stop | F9 (in the emulator) | F5 |
| step over | ⇧F8 | F10 |
| step back | F6 | ⇧F8 or F6 |
| reload | F4 | the **reset** button or *file → reload* (F4 is Run to cursor) |
