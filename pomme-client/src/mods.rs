//! Native (Rust-authored WebAssembly) mod discovery and lifecycle for Pomme.
//!
//! This handles only `pomme.mod.json` packages, not arbitrary native machine
//! code. Java/Fabric mods are a separate compatibility track handled by the
//! JVM integration layer.

#[cfg(feature = "native-mods")]
use pomme_mod::{PackageKind, WasmModRuntime, inspect_package};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredMod {
    pub path: std::path::PathBuf,
    pub id: String,
    pub module: String,
    pub permissions: Vec<String>,
}

#[derive(Debug, Default)]
pub struct ModLoadReport {
    pub discovered: Vec<DiscoveredMod>,
    pub errors: Vec<String>,
    pub executed_init: Vec<String>,
}

#[cfg(feature = "native-mods")]
pub fn discover_and_load_native_mods(mods_dir: &std::path::Path) -> ModLoadReport {
    let mut report = ModLoadReport::default();
    let entries = match std::fs::read_dir(mods_dir) {
        Ok(entries) => entries,
        Err(_) => return report,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("zip")
            && path.extension().and_then(|e| e.to_str()) != Some("pmod")
        {
            continue;
        }
        const MAX_PACKAGE_BYTES: usize = 64 * 1024 * 1024;
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => {
                if bytes.len() > MAX_PACKAGE_BYTES {
                    report
                        .errors
                        .push(format!("{}: package too large", path.display()));
                    continue;
                }
                bytes
            }
            Err(err) => {
                report.errors.push(format!("{}: {err}", path.display()));
                continue;
            }
        };
        let package = match inspect_package(&bytes) {
            Ok(package) => package,
            Err(err) => {
                report.errors.push(format!("{}: {err}", path.display()));
                continue;
            }
        };
        let metadata = match package.kind {
            PackageKind::NativeWasm(metadata) => metadata,
            PackageKind::FabricJava(_) => continue,
        };

        let module_bytes = match read_module_bytes(&bytes, &metadata.module) {
            Ok(bytes) => bytes,
            Err(err) => {
                report.errors.push(format!("{}: {err}", path.display()));
                continue;
            }
        };

        if metadata.permissions.iter().any(|p| p.trim().is_empty()) {
            report
                .errors
                .push(format!("{}: invalid permission", metadata.id));
            continue;
        }

        report.discovered.push(DiscoveredMod {
            path: path.clone(),
            id: metadata.id.clone(),
            module: metadata.module.clone(),
            permissions: metadata.permissions.clone(),
        });

        let runtime = WasmModRuntime::new();
        if let Err(err) = runtime.validate(&module_bytes) {
            report.errors.push(format!("{}: {err}", metadata.id));
            continue;
        }
        match runtime.call_unit_export(&module_bytes, "pomme_init", 1_000_000) {
            Ok(_) => report.executed_init.push(metadata.id),
            Err(err) => report.errors.push(format!("{}: {err}", metadata.id)),
        }
    }

    report
}

#[cfg(feature = "native-mods")]
fn read_module_bytes(archive_bytes: &[u8], module: &str) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let mut archive =
        zip::ZipArchive::new(std::io::Cursor::new(archive_bytes)).map_err(|e| e.to_string())?;
    let mut file = archive.by_name(module).map_err(|e| e.to_string())?;
    if file.size() as usize > 16 * 1024 * 1024 {
        return Err("module too large".to_string());
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    if bytes.len() > 16 * 1024 * 1024 {
        return Err("module too large".to_string());
    }
    Ok(bytes)
}

#[cfg(not(feature = "native-mods"))]
pub fn discover_and_load_native_mods(_mods_dir: &std::path::Path) -> ModLoadReport {
    ModLoadReport::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "native-mods")]
    #[test]
    fn ignores_non_native_archives() {
        let report = discover_and_load_native_mods(std::path::Path::new("/definitely/missing"));
        assert!(report.discovered.is_empty());
        assert!(report.errors.is_empty());
    }
}
