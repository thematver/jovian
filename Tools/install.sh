#!/bin/zsh
# Builds Jovian-8, installs it into ~/Applications and registers the Audio Unit.
set -e
cd "$(dirname "$0")/../Xcode"
tuist generate --no-open >/dev/null
xcodebuild -workspace Jovian8.xcworkspace -scheme Jovian8 -configuration Release -derivedDataPath build/DD build | grep -E "error:|BUILD" || true
BUILT="$PWD/build/DD/Build/Products/Release/Jovian8.app"
LSREG=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister
pluginkit -r "$BUILT/Contents/PlugIns/Jovian8AU.appex" 2>/dev/null || true
$LSREG -u "$BUILT" 2>/dev/null || true
mkdir -p ~/Applications
rm -rf ~/Applications/Jovian8.app
cp -R "$BUILT" ~/Applications/
$LSREG -f ~/Applications/Jovian8.app
pluginkit -a ~/Applications/Jovian8.app/Contents/PlugIns/Jovian8AU.appex
killall -9 AudioComponentRegistrar 2>/dev/null || true
echo "Installed ~/Applications/Jovian8.app"
