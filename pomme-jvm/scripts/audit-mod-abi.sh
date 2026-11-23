#!/bin/sh
set -eu
ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)"
RUNTIME="$(printenv POMME_FABRIC_RUNTIME 2>/dev/null || true)"
if [ -z "$RUNTIME" ]; then RUNTIME="$ROOT/pomme-ios/Resources/FabricRuntime/1.21.1"; fi
SHIM="$(printenv POMME_FABRIC_SHIM 2>/dev/null || true)"
if [ -z "$SHIM" ]; then SHIM="$RUNTIME/builtin-mods/pomme-fabric-api-shim.jar"; fi
JAVA_HOME="$(printenv JAVA_HOME 2>/dev/null || true)"
if [ -z "$JAVA_HOME" ]; then echo "Set JAVA_HOME to JDK 21" >&2; exit 2; fi
ASM="$RUNTIME/lib/asm-9.9.jar"
if [ "$#" -lt 1 ]; then echo "Usage: $0 <fabric-mod.jar> [more-mods.jar ...]" >&2; exit 2; fi
for file in "$SHIM" "$ASM"; do [ -s "$file" ] || { echo "Required runtime file missing: $file" >&2; exit 2; }; done
OUT="/tmp/pomme-mod-abi-audit"
mkdir -p "$OUT"
"$JAVA_HOME/bin/javac" --release 21 -proc:none -cp "$ASM" -d "$OUT" "$ROOT/pomme-jvm/tools/ModAbiAudit.java"
exec "$JAVA_HOME/bin/java" -cp "$OUT:$ASM" ModAbiAudit "$SHIM" "$@"
