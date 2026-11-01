# Installation

**English** · [Русский](Установка)

## Web version — nothing to install

Open **https://olegdurandin.github.io/RustEmu8086/** in Chrome, Safari, Firefox or Edge. It works on a
computer, an iPad and an Android tablet. The first visit downloads about 3 MB; after that the page opens from the
browser cache.

On a computer you get the Emu8086-style interface, on a tablet — the [touch layout](Tablet-layout). Programs,
settings and drive C: are kept in the browser — see [Web version](Web-version).

On an iPad you can add the site to the Home Screen: Safari → Share → Add to Home Screen.

## macOS app

Requires macOS 11 or later, on Apple Silicon or Intel.

1. Open [Releases](https://github.com/OlegDurandin/RustEmu8086/releases) and download
   `RustEmu8086-<version>-macos.zip`.
2. Unzip it and drag `RustEmu8086.app` to Applications.

### First launch

The app is not signed with an Apple certificate, so macOS warns that it cannot verify the developer. Open it in
one of these ways:

- **macOS 14 and earlier:** right-click (or Control-click) the app → **Open** → **Open**;
- **macOS 15 and later:** **System Settings → Privacy & Security** → **Open Anyway** at the bottom;
- **in Terminal:** `xattr -dr com.apple.quarantine /Applications/RustEmu8086.app`.

You only need to do this once.

### The latest build

A build of the latest `main` is in [Actions](https://github.com/OlegDurandin/RustEmu8086/actions): open the
latest successful **CI** run; the file *RustEmu8086-macos* is under *Artifacts* at the bottom. Downloading it
requires a GitHub login.

### Command line

The app contains the `rustemu` program for assembling and running without windows:

```sh
/Applications/RustEmu8086.app/Contents/MacOS/rustemu run hello.asm
```

See [Command line](Command-line).

## Windows and Linux

There are no ready-made builds yet — use the [web version](Web-version). The desktop app is built and tested on
macOS only; building on other systems has not been tried.
