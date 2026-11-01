#!/bin/sh
# Build the ABE iPhone app, bundle a scene into it, sign it and install it.
#
#   scripts/apple/build-ios.sh --sim    --world world.abew [--look look.json]
#   scripts/apple/build-ios.sh --device --world world.abew [--look look.json]
#
# --sim     builds for aarch64-apple-ios-sim, installs into the booted simulator (booting the
#           first available iPhone if none is) and launches the app with its console.
# --device  builds for aarch64-apple-ios, signs with your development team and installs on a
#           connected iPhone with devicectl, then launches it.
# --no-run  installs without launching.
#
# Environment:
#   ABE_IOS_TEAM       your Apple developer team ID (10 characters; device builds). Find it
#                      in Xcode > Settings > Accounts, or in a provisioning profile.
#   ABE_IOS_BUNDLE_ID  bundle identifier (default dev.abe.player.<team>); a development
#                      provisioning profile must cover it (see the README).
#   ABE_IOS_PROFILE    a .mobileprovision to embed (default: the newest installed profile for
#                      the team that covers the bundle identifier).
#   ABE_IOS_IDENTITY   codesign identity (default: the first "Apple Development" identity).
#   ABE_IOS_DEVICE     devicectl device identifier or name (default: the first paired device).
#   ABE_IOS_SIMULATOR  simulator name or UDID (default: the booted one, else the first iPhone).
#
# Needs Xcode (the iOS SDK and simulators, selected with xcode-select) and Rust (rustup).
set -eu
cd "$(dirname "$0")/../.."

mode=""
world=""
look=""
run=1
while [ $# -gt 0 ]; do
    case "$1" in
    --sim) mode=sim ;;
    --device) mode=device ;;
    --world)
        world="$2"
        shift
        ;;
    --look)
        look="$2"
        shift
        ;;
    --no-run) run=0 ;;
    -h | --help)
        sed -n '2,25p' "$0"
        exit 0
        ;;
    *)
        echo "unknown option $1" >&2
        exit 2
        ;;
    esac
    shift
done
die() {
    echo "build-ios.sh: $*" >&2
    exit 1
}
[ -n "$mode" ] || die "pass --sim or --device (see --help)"
[ -n "$world" ] || die "pass --world path/to/world.abew (cook one with abe-build)"
[ -f "$world" ] || die "$world does not exist"
[ -z "$look" ] || [ -f "$look" ] || die "$look does not exist"
[ "$(uname -s)" = Darwin ] || die "runs on macOS"
xcrun --sdk iphoneos --show-sdk-path >/dev/null 2>&1 ||
    die "the iOS SDK is missing: install Xcode and run sudo xcode-select -s /Applications/Xcode.app"

team="${ABE_IOS_TEAM:-}"
if [ "$mode" = device ] && [ -z "$team" ]; then
    die "set ABE_IOS_TEAM to your development team ID"
fi
bundle="${ABE_IOS_BUNDLE_ID:-dev.abe.player${team:+.$(echo "$team" | tr 'A-Z' 'a-z')}}"
if [ "$mode" = sim ]; then
    target=aarch64-apple-ios-sim
    platform=iPhoneSimulator
else
    target=aarch64-apple-ios
    platform=iPhoneOS
fi
command -v rustup >/dev/null 2>&1 && rustup target add "$target" >/dev/null

# The app uses scenes (iOS 13) and is tested against iOS 15 and later.
export IPHONEOS_DEPLOYMENT_TARGET=15.0
cargo build --release --locked --target "$target" --bin abe-ios
out="${CARGO_TARGET_DIR:-target}/$target/release"
app="$out/ABE.app"
rm -rf "$app"
mkdir -p "$app"
cp "$out/abe-ios" "$app/abe-ios"
cp "$world" "$app/world.abew"
[ -z "$look" ] || cp "$look" "$app/look.json"

cat >"$app/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key><string>en</string>
    <key>CFBundleDisplayName</key><string>ABE</string>
    <key>CFBundleExecutable</key><string>abe-ios</string>
    <key>CFBundleIdentifier</key><string>$bundle</string>
    <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
    <key>CFBundleName</key><string>ABE</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>0.2.0</string>
    <key>CFBundleVersion</key><string>1</string>
    <key>CFBundleSupportedPlatforms</key><array><string>$platform</string></array>
    <key>LSRequiresIPhoneOS</key><true/>
    <key>MinimumOSVersion</key><string>$IPHONEOS_DEPLOYMENT_TARGET</string>
    <key>UIDeviceFamily</key><array><integer>1</integer><integer>2</integer></array>
    <key>UIRequiredDeviceCapabilities</key><array><string>arm64</string><string>metal</string></array>
    <key>UILaunchScreen</key><dict/>
    <key>UIStatusBarHidden</key><true/>
    <key>UIRequiresFullScreen</key><true/>
    <key>CADisableMinimumFrameDurationOnPhone</key><true/>
    <key>UISupportedInterfaceOrientations</key>
    <array>
        <string>UIInterfaceOrientationLandscapeLeft</string>
        <string>UIInterfaceOrientationLandscapeRight</string>
        <string>UIInterfaceOrientationPortrait</string>
    </array>
    <key>UIApplicationSceneManifest</key>
    <dict>
        <key>UIApplicationSupportsMultipleScenes</key><false/>
        <key>UISceneConfigurations</key>
        <dict>
            <key>UIWindowSceneSessionRoleApplication</key>
            <array>
                <dict>
                    <key>UISceneConfigurationName</key><string>Default</string>
                    <key>UISceneDelegateClassName</key><string>ABESceneDelegate</string>
                </dict>
            </array>
        </dict>
    </dict>
</dict>
</plist>
EOF
plutil -lint "$app/Info.plist" >/dev/null

if [ "$mode" = sim ]; then
    codesign --force --sign - --timestamp=none "$app"
    sim="${ABE_IOS_SIMULATOR:-}"
    if [ -z "$sim" ]; then
        if xcrun simctl list devices booted | grep -q Booted; then
            sim=booted
        else
            sim=$(xcrun simctl list devices available | grep -m1 -E '^[[:space:]]+iPhone' |
                sed -E 's/.*\(([0-9A-F-]{36})\).*/\1/')
            [ -n "$sim" ] || die "no iPhone simulator is installed (Xcode > Settings > Components)"
            xcrun simctl boot "$sim"
            open -a Simulator
        fi
    fi
    xcrun simctl install "$sim" "$app"
    echo "Installed $bundle in the simulator ($sim)."
    [ "$run" = 1 ] && exec xcrun simctl launch --console-pty "$sim" "$bundle"
    exit 0
fi

# Device: embed a development provisioning profile and sign with its entitlements.
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
profile="${ABE_IOS_PROFILE:-}"
if [ -z "$profile" ]; then
    best=""
    for dir in "$HOME/Library/Developer/Xcode/UserData/Provisioning Profiles" \
        "$HOME/Library/MobileDevice/Provisioning Profiles"; do
        [ -d "$dir" ] || continue
        for p in "$dir"/*.mobileprovision; do
            [ -f "$p" ] || continue
            security cms -D -i "$p" >"$tmp/candidate.plist" 2>/dev/null || continue
            t=$(/usr/libexec/PlistBuddy -c 'Print :TeamIdentifier:0' "$tmp/candidate.plist" 2>/dev/null || true)
            id=$(/usr/libexec/PlistBuddy -c 'Print :Entitlements:application-identifier' "$tmp/candidate.plist" 2>/dev/null || true)
            [ "$t" = "$team" ] || continue
            case "$id" in
            "$team.$bundle") best="$p" && break 2 ;;
            "$team.*") [ -n "$best" ] || best="$p" ;;
            esac
        done
    done
    profile="$best"
fi
[ -n "$profile" ] || die "no provisioning profile of team $team covers $bundle; see the README \
(open Xcode once with this bundle identifier, or set ABE_IOS_PROFILE)"
cp "$profile" "$app/embedded.mobileprovision"
security cms -D -i "$profile" >"$tmp/profile.plist"
/usr/libexec/PlistBuddy -x -c 'Print :Entitlements' "$tmp/profile.plist" >"$tmp/entitlements.plist"
/usr/libexec/PlistBuddy -c "Set :application-identifier $team.$bundle" "$tmp/entitlements.plist"

identity="${ABE_IOS_IDENTITY:-}"
if [ -z "$identity" ]; then
    identity=$(security find-identity -v -p codesigning | grep -m1 'Apple Development' |
        sed -E 's/^ *[0-9]+\) ([0-9A-F]{40}).*/\1/')
fi
[ -n "$identity" ] || die "no Apple Development signing identity in the keychain (sign in to Xcode)"
codesign --force --sign "$identity" --entitlements "$tmp/entitlements.plist" --timestamp=none "$app"

device="${ABE_IOS_DEVICE:-}"
if [ -z "$device" ]; then
    xcrun devicectl list devices --json-output "$tmp/devices.json" >/dev/null
    device=$(plutil -extract result.devices.0.identifier raw "$tmp/devices.json" 2>/dev/null || true)
fi
[ -n "$device" ] || die "no paired iPhone found (connect it, trust this Mac, enable Developer Mode)"
xcrun devicectl device install app --device "$device" "$app"
echo "Installed $bundle on $device."
if [ "$run" = 1 ]; then
    xcrun devicectl device process launch --device "$device" "$bundle"
fi
