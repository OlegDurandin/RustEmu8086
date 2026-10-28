#!/bin/sh
# Builds RustEmu8086.app for the iPad simulator, installs it on the booted simulator (or the one
# named in $SIM, e.g. SIM="iPad Air 13-inch (M4)") and launches it.
# Needs Xcode with an iOS simulator and: rustup target add aarch64-apple-ios-sim
# RELEASE=1 builds an optimized binary; NOLAUNCH=1 only builds the bundle.
set -e
cd "$(dirname "$0")/.."
TARGET=aarch64-apple-ios-sim
PROFILE=debug
FLAGS=
if [ "$RELEASE" = 1 ]; then PROFILE=release; FLAGS=--release; fi
cargo build $FLAGS --target $TARGET -p rustemu-tablet --bin rustemu-tablet

BUNDLE=target/$TARGET/$PROFILE/bundle/RustEmu8086.app
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
rm -rf "$BUNDLE"
mkdir -p "$BUNDLE"
cp "target/$TARGET/$PROFILE/rustemu-tablet" "$BUNDLE/"

# Icon (iPad sizes)
ICON=target/$TARGET/icon_1024.png
[ -f "$ICON" ] || cargo run --quiet --release -p rustemu-ui --example make_icon -- "$ICON"
sips -z 152 152 "$ICON" --out "$BUNDLE/AppIcon76x76@2x~ipad.png" >/dev/null
sips -z 167 167 "$ICON" --out "$BUNDLE/AppIcon83.5x83.5@2x~ipad.png" >/dev/null

# UIApplicationSceneManifest: iPadOS 27 requires the scene life cycle (the window is attached to
# the scene in crates/rustemu-tablet/src/ios.rs). The status bar is hidden: the menu bar takes the top edge.
cat > "$BUNDLE/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleIdentifier</key><string>io.github.olegdurandin.rustemu8086</string>
  <key>CFBundleName</key><string>RustEmu8086</string>
  <key>CFBundleDisplayName</key><string>RustEmu8086</string>
  <key>CFBundleExecutable</key><string>rustemu-tablet</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>CFBundleIcons~ipad</key>
  <dict>
    <key>CFBundlePrimaryIcon</key>
    <dict>
      <key>CFBundleIconFiles</key>
      <array><string>AppIcon76x76</string><string>AppIcon83.5x83.5</string></array>
    </dict>
  </dict>
  <key>MinimumOSVersion</key><string>15.0</string>
  <key>UIDeviceFamily</key><array><integer>2</integer></array>
  <key>UILaunchScreen</key><dict/>
  <key>UIStatusBarHidden</key><true/>
  <key>UIViewControllerBasedStatusBarAppearance</key><false/>
  <key>UIApplicationSceneManifest</key>
  <dict>
    <key>UIApplicationSupportsMultipleScenes</key><false/>
    <key>UISceneConfigurations</key>
    <dict>
      <key>UIWindowSceneSessionRoleApplication</key>
      <array><dict><key>UISceneConfigurationName</key><string>Default</string></dict></array>
    </dict>
  </dict>
  <key>UISupportedInterfaceOrientations~ipad</key>
  <array>
    <string>UIInterfaceOrientationLandscapeLeft</string>
    <string>UIInterfaceOrientationLandscapeRight</string>
    <string>UIInterfaceOrientationPortrait</string>
    <string>UIInterfaceOrientationPortraitUpsideDown</string>
  </array>
  <key>UIFileSharingEnabled</key><true/>
  <key>LSSupportsOpeningDocumentsInPlace</key><true/>
</dict>
</plist>
EOF
codesign -s - --force "$BUNDLE" >/dev/null 2>&1
echo "Built $BUNDLE"
[ "$NOLAUNCH" = 1 ] && exit 0

DEVICE=${SIM:-booted}
if [ -n "$SIM" ]; then
  xcrun simctl boot "$SIM" 2>/dev/null || true
  open -a Simulator
fi
xcrun simctl install "$DEVICE" "$BUNDLE"
xcrun simctl launch --terminate-running-process "$DEVICE" io.github.olegdurandin.rustemu8086
