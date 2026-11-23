#!/bin/sh
set -eu
ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)"
if [ "$#" -ge 1 ]; then MC_VERSION="$1"; else MC_VERSION="1.21.1"; fi
DEST="$ROOT/pomme-ios/Resources/FabricRuntime/$MC_VERSION"
JAVA_HOME="$(printenv JAVA_HOME 2>/dev/null || true)"
if [ -z "$JAVA_HOME" ]; then echo "Set JAVA_HOME to JDK 21" >&2; exit 2; fi
BRIDGE="$DEST/builtin-mods/pomme-fabric-api-shim.jar"
OUT="/tmp/pomme-minecraft-api-verifier"
mkdir -p "$OUT"
"$JAVA_HOME/bin/javac" --release 21 -proc:none -d "$OUT" "$ROOT/pomme-jvm/tools/VerifyMinecraftApiStubs.java"
exec "$JAVA_HOME/bin/java" -Xmx2g -cp "$OUT" VerifyMinecraftApiStubs "$BRIDGE" "$DEST/lib"
