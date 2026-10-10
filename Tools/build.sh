#!/bin/zsh
# Builds Jovian-8 as CLAP, VST3 and AUv2 and installs them into ~/Library/Audio/Plug-Ins.
#   Tools/build.sh            build + install
#   Tools/build.sh --check    also run auval
set -e
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
PLUG=~/Library/Audio/Plug-Ins
IDENTITY="${JOVIAN_SIGN_IDENTITY:-$(security find-identity -v -p codesigning | awk -F'"' '/Apple Development/ {print $2; exit}')}"
IDENTITY="${IDENTITY:--}"

# clap-wrapper at a pinned commit, with our fixes (au/clap-wrapper.patch)
CW=third_party/clap-wrapper
CW_REV=1cca996e96f29ab2be7ae9f8cfe532bbc92e1dd6
if [ ! -d $CW ]; then
  git clone -q https://github.com/free-audio/clap-wrapper.git $CW
  git -C $CW checkout -q $CW_REV
  git -C $CW apply "$ROOT/au/clap-wrapper.patch"
fi

echo "▸ Rust (CLAP, VST3, static library for AU)"
cargo xtask bundle jovian-plugin --release 2>&1 | grep -E "^(error|Created)" || true
test -f target/release/libjovian_plugin.a

echo "▸ AUv2 (clap-wrapper)"
if [ ! -f build/au/build.ninja ]; then
  cmake -S au -B build/au -G Ninja -DCMAKE_BUILD_TYPE=Release -DJOVIAN_STATICLIB="$ROOT/target/release/libjovian_plugin.a" >/dev/null
fi
cmake --build build/au 2>&1 | grep -E "error|FAILED" || true
test -d build/au/Jovian-8.component

echo "▸ Sign ($IDENTITY) and install"
for b in target/bundled/Jovian-8.clap target/bundled/Jovian-8.vst3 build/au/Jovian-8.component; do
  codesign --force --deep --timestamp=none -s "$IDENTITY" "$b" 2>/dev/null
done
mkdir -p $PLUG/CLAP $PLUG/VST3 $PLUG/Components
rm -rf $PLUG/CLAP/Jovian-8.clap $PLUG/VST3/Jovian-8.vst3 $PLUG/Components/Jovian-8.component
cp -R target/bundled/Jovian-8.clap $PLUG/CLAP/
cp -R target/bundled/Jovian-8.vst3 $PLUG/VST3/
cp -R build/au/Jovian-8.component $PLUG/Components/
killall -9 AudioComponentRegistrar 2>/dev/null || true
echo "Installed: $PLUG/Components/Jovian-8.component, VST3/Jovian-8.vst3, CLAP/Jovian-8.clap"

if [ "$1" = "--check" ]; then
  auval -v aumu JV8e Jovn 2>&1 | grep -E "AU VALIDATION|FAIL" | tail -3
fi
