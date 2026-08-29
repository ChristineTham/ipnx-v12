#!/bin/sh
# M3: package the native host as IPNX.app — the binary, the rootfs, and a
# launcher that boots the windowed shell (init -w: rc in a #w window; no tty
# anywhere). Build products land in hosts/macos/build/.
set -e
cd "$(dirname "$0")/../.."
cargo build --release -p host
APP=hosts/macos/build/IPNX.app
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/host "$APP/Contents/MacOS/host"
cp -R userspace/rootfs "$APP/Contents/Resources/rootfs"
cat > "$APP/Contents/MacOS/IPNX" <<'SH'
#!/bin/sh
here="$(cd "$(dirname "$0")/.." && pwd)"
exec "$here/MacOS/host" "$here/Resources/rootfs" --app
SH
chmod +x "$APP/Contents/MacOS/IPNX" "$APP/Contents/MacOS/host"
cat > "$APP/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
	<key>CFBundleName</key><string>IPNX</string>
	<key>CFBundleDisplayName</key><string>IPNX</string>
	<key>CFBundleIdentifier</key><string>net.christham.ipnx</string>
	<key>CFBundleExecutable</key><string>IPNX</string>
	<key>CFBundleVersion</key><string>12.0</string>
	<key>CFBundleShortVersionString</key><string>12.0</string>
	<key>CFBundlePackageType</key><string>APPL</string>
	<key>NSHighResolutionCapable</key><true/>
	<key>LSMinimumSystemVersion</key><string>13.0</string>
</dict></plist>
PLIST
echo "built: $APP ($(du -sh "$APP" | awk '{print $1}'))"
