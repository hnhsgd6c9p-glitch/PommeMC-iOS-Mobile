#!/bin/sh
set -eu
cd "$(dirname "$0")"
command -v javac >/dev/null 2>&1 || { echo "JDK 21 is required (javac not found)." >&2; exit 1; }
command -v jar >/dev/null 2>&1 || { echo "JDK 21 is required (jar not found)." >&2; exit 1; }
rm -rf build
mkdir -p build/stubs build/classes
javac --release 21 -d build/stubs stubs/pomme/api/Pomme.java stubs/net/fabricmc/api/ModInitializer.java
javac --release 21 -classpath build/stubs -d build/classes src/example/ExampleMod.java
cp fabric.mod.json build/classes/
jar --create --file build/tiny-example.jar -C build/classes example -C build/classes fabric.mod.json
printf 'Created %s/build/tiny-example.jar\n' "$PWD"
