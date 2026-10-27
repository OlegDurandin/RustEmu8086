#!/bin/sh
# Builds RustEmu8086.app and RustEmu8086-<version>-macos.zip in target/release/bundle/.
# UNIVERSAL=1 builds one binary for both Apple Silicon and Intel Macs
# (needs: rustup target add aarch64-apple-darwin x86_64-apple-darwin).
set -e
cd "$(dirname "$0")/.."
BUNDLE=target/release/bundle/RustEmu8086.app
rm -rf "$BUNDLE"
mkdir -p "$BUNDLE/Contents/MacOS" "$BUNDLE/Contents/Resources"
if [ "$UNIVERSAL" = 1 ]; then
  for t in aarch64-apple-darwin x86_64-apple-darwin; do
    cargo build --release --target $t -p rustemu-app -p rustemu-cli
  done
  for b in RustEmu8086 rustemu; do
    lipo -create -output "$BUNDLE/Contents/MacOS/$b" \
      "target/aarch64-apple-darwin/release/$b" "target/x86_64-apple-darwin/release/$b"
  done
else
  cargo build --release -p rustemu-app -p rustemu-cli
  cp target/release/RustEmu8086 "$BUNDLE/Contents/MacOS/RustEmu8086"
  cp target/release/rustemu "$BUNDLE/Contents/MacOS/rustemu"
fi

# Icon
ICONSET=target/release/bundle/RustEmu8086.iconset
rm -rf "$ICONSET" && mkdir -p "$ICONSET"
cargo run --quiet --release -p rustemu-app --example make_icon -- target/release/bundle/icon_1024.png
for s in 16 32 128 256 512; do
  sips -z $s $s target/release/bundle/icon_1024.png --out "$ICONSET/icon_${s}x${s}.png" >/dev/null
  d=$((s * 2))
  sips -z $d $d target/release/bundle/icon_1024.png --out "$ICONSET/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$BUNDLE/Contents/Resources/RustEmu8086.icns"

VERSION=$(grep -m1 '^version' Cargo.toml | sed 's/.*"\(.*\)"/\1/')
cat > "$BUNDLE/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>RustEmu8086</string>
  <key>CFBundleDisplayName</key><string>RustEmu8086</string>
  <key>CFBundleIdentifier</key><string>io.github.rustemu8086</string>
  <key>CFBundleVersion</key><string>${VERSION}</string>
  <key>CFBundleShortVersionString</key><string>${VERSION}</string>
  <key>CFBundleExecutable</key><string>RustEmu8086</string>
  <key>CFBundleIconFile</key><string>RustEmu8086</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>CFBundleDocumentTypes</key>
  <array>
    <dict>
      <key>CFBundleTypeName</key><string>8086 Assembly Source</string>
      <key>CFBundleTypeExtensions</key><array><string>asm</string><string>nasm</string><string>inc</string></array>
      <key>CFBundleTypeRole</key><string>Editor</string>
    </dict>
  </array>
</dict>
</plist>
PLIST
# Ad-hoc signature: required to run on Apple Silicon (there is no Developer ID / notarization).
codesign --force --deep --sign - "$BUNDLE"
ZIP="target/release/bundle/RustEmu8086-${VERSION}-macos.zip"
rm -f "$ZIP"
ditto -c -k --keepParent "$BUNDLE" "$ZIP"
echo "Built $BUNDLE"
echo "Archive $ZIP"
