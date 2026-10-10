#!/bin/zsh
# Builds Jovian-8 as CLAP, VST3 and AUv2. Installs only after every build succeeds.
#   Tools/build.sh                 build + install, preserving the previous bundles
#   Tools/build.sh --check         also run auval after installation
#   Tools/build.sh --build-only    build without changing installed plugins
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
PLUG="${JOVIAN_PLUGIN_DIR:-$HOME/Library/Audio/Plug-Ins}"
CHECK=false
INSTALL=true
for arg in "$@"; do
  case "$arg" in
    --check) CHECK=true ;;
    --build-only) INSTALL=false ;;
    *) echo "Unknown option: $arg" >&2; exit 2 ;;
  esac
done
if $CHECK && ! $INSTALL; then
  echo "--check validates the installed AU; use it without --build-only." >&2
  exit 2
fi
mkdir -p build/logs

# clap-wrapper at a pinned commit, with our fixes (au/clap-wrapper.patch)
CW=third_party/clap-wrapper
CW_REV=1cca996e96f29ab2be7ae9f8cfe532bbc92e1dd6
if [ ! -d "$CW" ]; then
  git clone -q https://github.com/free-audio/clap-wrapper.git "$CW"
  git -C "$CW" checkout -q "$CW_REV"
  git -C "$CW" apply "$ROOT/au/clap-wrapper.patch"
fi

echo "▸ Rust (CLAP, VST3, static library for AU)"
# pipefail preserves the compiler exit code; full diagnostics remain in the log.
cargo xtask bundle jovian-plugin --release 2>&1 | tee build/logs/rust-bundle.log
test -s target/release/libjovian_plugin.a
test -d target/bundled/Jovian-8.clap
test -d target/bundled/Jovian-8.vst3

echo "▸ AUv2 (clap-wrapper)"
cmake -S au -B build/au -G Ninja -DCMAKE_BUILD_TYPE=Release -DJOVIAN_STATICLIB="$ROOT/target/release/libjovian_plugin.a" 2>&1 | tee build/logs/au-configure.log
cmake --build build/au 2>&1 | tee build/logs/au-build.log
test -d build/au/Jovian-8.component

if ! $INSTALL; then
  echo "Built: target/bundled/Jovian-8.clap, target/bundled/Jovian-8.vst3, build/au/Jovian-8.component"
  exit 0
fi

IDENTITY="${JOVIAN_SIGN_IDENTITY:-$(security find-identity -v -p codesigning | awk -F'"' '/Apple Development/ {print $2; exit}')}"
IDENTITY="${IDENTITY:--}"
echo "▸ Sign ($IDENTITY) and install"
mkdir -p "$PLUG/CLAP" "$PLUG/VST3" "$PLUG/Components"
STAGE="$(mktemp -d "$PLUG/.jovian-stage.XXXXXX")"
BACKUP="$PLUG/.jovian-backups/$(date -u +%Y%m%dT%H%M%SZ)-$$"
typeset -a SOURCES FORMATS NAMES INSTALLED SAVED
SOURCES=(target/bundled/Jovian-8.clap target/bundled/Jovian-8.vst3 build/au/Jovian-8.component)
FORMATS=(CLAP VST3 Components)
NAMES=(Jovian-8.clap Jovian-8.vst3 Jovian-8.component)
INSTALLED=()
SAVED=()
COMPLETE=false

cleanup() {
  local result=$?
  set +e
  if ! $COMPLETE; then
    # Roll back all formats if any replacement fails. Old bundles are renamed,
    # never deleted before the replacement has been staged and signed.
    for rel in "${INSTALLED[@]}"; do rm -rf "$PLUG/$rel"; done
    for rel in "${SAVED[@]}"; do mv "$BACKUP/$rel" "$PLUG/$rel"; done
    [ ${#SAVED[@]} -eq 0 ] || echo "Installation failed; restored the previous plugins." >&2
  fi
  rm -rf "$STAGE"
  return "$result"
}
trap cleanup EXIT

for i in 1 2 3; do
  mkdir -p "$STAGE/${FORMATS[$i]}"
  cp -R "${SOURCES[$i]}" "$STAGE/${FORMATS[$i]}/${NAMES[$i]}"
  codesign --force --deep --timestamp=none -s "$IDENTITY" "$STAGE/${FORMATS[$i]}/${NAMES[$i]}"
done
for i in 1 2 3; do
  rel="${FORMATS[$i]}/${NAMES[$i]}"
  if [ -e "$PLUG/$rel" ]; then
    mkdir -p "$BACKUP/${FORMATS[$i]}"
    mv "$PLUG/$rel" "$BACKUP/$rel"
    SAVED+=("$rel")
  fi
  mv "$STAGE/$rel" "$PLUG/$rel"
  INSTALLED+=("$rel")
done
COMPLETE=true
killall AudioComponentRegistrar 2>/dev/null || true
echo "Installed: $PLUG/Components/Jovian-8.component, VST3/Jovian-8.vst3, CLAP/Jovian-8.clap"
if [ ${#SAVED[@]} -gt 0 ]; then echo "Previous versions: $BACKUP"; fi

if $CHECK; then
  # AudioComponentRegistrar restarts asynchronously. Retry only a missing
  # registration; genuine validation failures keep their exit status.
  for attempt in 1 2 3 4 5; do
    validation_result=0
    auval -v aumu JV8e Jovn 2>&1 | tee build/logs/auval.log || validation_result=$?
    if [ "$validation_result" -eq 0 ]; then break; fi
    if [ "$attempt" -eq 5 ] || ! grep -q "FATAL ERROR: didn't find the component" build/logs/auval.log; then
      exit "$validation_result"
    fi
    sleep 1
  done
fi
