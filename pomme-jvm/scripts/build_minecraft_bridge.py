#!/usr/bin/env python3
"""Split Minecraft ABI bridge classes out of the Pomme Fabric API shim JAR."""
import sys
import zipfile
from pathlib import Path

source = Path(sys.argv[1])
destination = Path(sys.argv[2])
if not source.is_file():
    raise SystemExit(f"missing Pomme Fabric shim: {source}")
destination.parent.mkdir(parents=True, exist_ok=True)
include_prefixes = ("net/minecraft/", "com/mojang/", "pomme/bridge/")
copied = 0
with zipfile.ZipFile(source, "r") as src, zipfile.ZipFile(destination, "w", zipfile.ZIP_DEFLATED) as dst:
    for info in src.infolist():
        name = info.filename
        if name.endswith(".class") and name.startswith(include_prefixes):
            dst.writestr(info, src.read(name))
            copied += 1
    metadata = b'''{\n  "schemaVersion": 1,\n  "id": "pomme-minecraft-api-bridge",\n  "version": "1.21.1-pomme.1",\n  "name": "Pomme Minecraft API Bridge",\n  "environment": "*"\n}\n'''
    dst.writestr("fabric.mod.json", metadata)
if copied < 8000:
    destination.unlink(missing_ok=True)
    raise SystemExit(f"expected at least 8000 Minecraft bridge classes, found {copied}")
print(f"Built {destination} with {copied} Minecraft/Pomme bridge classes")
