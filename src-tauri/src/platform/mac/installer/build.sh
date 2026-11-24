#!/usr/bin/env bash
# Build de Mac: ContextSwitch.app + ContextSwitch_<versión>_<arq>.dmg en release/mac/.
# Uso (desde la raíz del repo): npm run build:mac
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../../../../.." && pwd)"
cd "$ROOT"
export PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH"

# Tauri valida que el recurso cs-bridge exista ANTES de compilarlo: un marcador vacío
# alcanza; beforeBuildCommand lo reemplaza por el binario real.
mkdir -p src-tauri/target/release
[ -f src-tauri/target/release/cs-bridge ] || touch src-tauri/target/release/cs-bridge

npm run tauri build -- --bundles app,dmg

OUT="release/mac"
rm -rf "$OUT" && mkdir -p "$OUT"
cp -R src-tauri/target/release/bundle/macos/ContextSwitch.app "$OUT/"
cp src-tauri/target/release/bundle/dmg/*.dmg "$OUT/"
echo "Listo: $OUT"
ls -1 "$OUT"
