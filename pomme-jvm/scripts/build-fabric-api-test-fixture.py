#!/usr/bin/env python3
"""Build a test-only official Fabric API aggregate containing a mod's API closure."""

from __future__ import annotations

import argparse
import io
import json
import zipfile
from pathlib import Path

SEED_MODULES = {"fabric-api-base", "fabric-networking-api-v1"}


def read_module_metadata(data: bytes) -> dict:
    with zipfile.ZipFile(io.BytesIO(data)) as archive:
        return json.loads(archive.read("fabric.mod.json"))


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("official_aggregate", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()

    with zipfile.ZipFile(args.official_aggregate) as source:
        aggregate = json.loads(source.read("fabric.mod.json"))
        nested = {entry["file"] for entry in aggregate.get("jars", [])}
        modules: dict[str, tuple[str, dict]] = {}
        for path in sorted(nested):
            metadata = read_module_metadata(source.read(path))
            modules[metadata["id"]] = (path, metadata)

        missing = SEED_MODULES - modules.keys()
        if missing:
            raise SystemExit(f"official Fabric API aggregate lacks required modules: {sorted(missing)}")

        selected = set(SEED_MODULES)
        pending = list(SEED_MODULES)
        while pending:
            module_id = pending.pop()
            metadata = modules[module_id][1]
            for dependency in metadata.get("depends", {}):
                if dependency in modules and dependency not in selected:
                    selected.add(dependency)
                    pending.append(dependency)

        selected_paths = [modules[module_id][0] for module_id in modules if module_id in selected]
        aggregate["jars"] = [{"file": path} for path in selected_paths]

        args.output.parent.mkdir(parents=True, exist_ok=True)
        with zipfile.ZipFile(args.output, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=6) as target:
            for item in source.infolist():
                if item.filename.startswith("META-INF/jars/") and item.filename not in selected_paths:
                    continue
                data = json.dumps(aggregate, indent=2).encode() if item.filename == "fabric.mod.json" else source.read(item.filename)
                target.writestr(item, data)

    print(f"Built official Fabric API test aggregate with {len(selected)} modules: {', '.join(sorted(selected))}")
    print(f"Output: {args.output}")


if __name__ == "__main__":
    main()
