# Web version

**English** · [Русский](Веб-версия)

**https://olegdurandin.github.io/RustEmu8086/** is the same app built for WebAssembly. It runs in Chrome,
Safari, Firefox and Edge on a computer, an iPad and Android, with no installation and no sign-up. Nothing is sent
to a server: programs, settings and files stay in your browser.

## Layout

On a computer you get the Emu8086-style interface. On an iPad (including with a keyboard and trackpad), other
touch devices and in a narrow window — the [tablet layout](Tablet-layout).

You can switch without losing the program: the **options** window → *layout*, in the tablet layout — the ⋮ menu →
*Desktop layout*. The choice is remembered. For a single visit: `?ui=desktop` or `?ui=tablet` in the address.

## My programs

- **Save** (⌘S) saves the program in the browser; a new one asks for a name first.
- **Open** (⌘O) opens the *my programs* window: a list (most recent first), open, download, delete.
- **upload from computer…** — a file from the computer: an `.asm` opens and is added to the list, a `.com` /
  `.exe` / `.bin` is loaded into the emulator.
- **File → Download** — download the program as a file; **compile** — download the assembled `.com` / `.exe`.

**Nothing is lost.** What is open in the editor, unsaved edits included, comes back on your next visit.
Everything is saved every 10 seconds and when the tab is closed. If the browser does not let the site store data
(some private modes) or the site's storage (about 5 MB) is full, the app warns about it and asks before you close
a tab with unsaved edits. When storage is full, free some space — delete unneeded files from drive C: or old
programs — and saving retries by itself.

> The data lives in this browser on this device. Clearing the site data deletes it. Download important programs
> as files.

## Drive C:

DOS programs (INT 21h: files, directories, search, rename) work with a drive C: kept in browser storage; it
persists between visits. The **drive C:** window — *Emulator → Virtual drive → drive C: files…*, in the tablet
layout — the ⋮ menu → *Drive C: files*:

- **upload files…** — files the program will read;
- view, download and delete the files it has written.

The capacity is 2 MB for all files and up to 1024 files and directories. On a full drive a write, as in DOS,
writes fewer bytes than requested.

## Sharing a program

The **share** button on the toolbar (or **File → Share link…**; in the tablet layout — the icon next to the
keyboard) gives a link that opens the program from the editor on the site. The *run the program when the link is
opened* checkbox starts it right away.

- The program is compressed right into the link (`#name=…&code=…`). The browser does not send the part after `#`
  to the server, so the code is not uploaded anywhere.
- Handy for teaching: the teacher sends a link, and the example is already open for the student.
- The opened program is a new file: **Save** keeps it in "my programs". If the editor had unsaved edits, the app
  asks first.
- Some messengers cut links longer than 8000 characters — send a file then.
- The Mac app can make a link too — it leads to the site.

**Someone else's program does not see your files.** Until a program from a link is saved, it works with an empty
temporary drive C:, and whatever it writes there is not kept. A window says so on the first run, and so does a
note in the **drive C:** window. To run the program with your drive, save it and run it again.

## Links to examples

- `?example=palindrome.asm` — open a built-in example (the name is case-insensitive; the old names of our former
  examples, such as `bubble_sort.asm`, open the Emu8086 examples that replaced them; an unknown name shows a
  message);
- `&run` — run it right away: `https://olegdurandin.github.io/RustEmu8086/?example=hello.asm&run`.

## How it differs from the Mac app

- Drive C: and programs live in the browser, not as files on the computer.
- The browser does not pass ⌘N, ⌘W and ⌘T to the page — use the menu. F1–F10, Esc, ⌘R, ⌘F, ⌘G, ⌘L, ⌘B, ⌘E work as
  in the app (reload the page with ⌘⇧R).
- Windows with CP437 text (the ASCII table, the memory dump) lack four characters: ☻ ☼ ⌂ ⁿ. The emulator screen
  draws all of them.
- The emulator is about 1.5–2 times slower — tens of millions of instructions per second, still far more than an
  8086 needs.

## If it loads slowly

The first visit downloads about 3 MB; after every site update — again, otherwise from the cache. If loading is
slow every time, see [Troubleshooting](Troubleshooting#the-site-loads-slowly).
