//! Safe package inspection and manifest handling for Pomme's Rust-native mod
//! system.
//!
//! Fabric metadata and Java class files are inspected before a mod is enabled.
//! A deliberately restricted, fuel-metered integer bytecode interpreter is
//! available for isolated helper methods; it is not a JVM or Fabric loader.
//! Native Pomme modules use a separate WebAssembly payload and sandbox.

use std::collections::BTreeMap;
use std::io::{Cursor, Read};
use std::path::{Component, Path};

use serde::Deserialize;
use thiserror::Error;
use zip::ZipArchive;

pub mod classfile;
pub mod java_subset;
mod wasm_runtime;
pub use classfile::{
    ClassAnalysis, ClassFileError, JarCompatibilityReport, MethodReference, MethodSummary,
    analyze_class, analyze_jar_classes,
};
pub use java_subset::{JavaSubsetError, execute_static_i32};
pub use wasm_runtime::{WasmModRuntime, WasmRuntimeError};

const FABRIC_MANIFEST: &str = "fabric.mod.json";
const POMME_MANIFEST: &str = "pomme.mod.json";
const MAX_ARCHIVE_ENTRIES: usize = 10_000;
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
/// Hard cap on total decompressed .class bytes read from one JAR.
const MAX_TOTAL_CLASS_BYTES: u64 = 64 * 1024 * 1024;
/// Hard cap on the number of .class entries analyzed from one JAR.
const MAX_CLASS_FILES: usize = 4_000;
/// Per-class size limit (matches classfile::MAX_CLASS_BYTES).
const MAX_SINGLE_CLASS_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum ModError {
    #[error("invalid ZIP/JAR archive: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("invalid Java class file: {0}")]
    ClassFile(#[from] ClassFileError),
    #[error("invalid JSON manifest: {0}")]
    Json(#[from] serde_json::Error),
    #[error("archive contains too many entries ({0}; maximum {MAX_ARCHIVE_ENTRIES})")]
    TooManyEntries(usize),
    #[error("archive contains too many class files ({0}; maximum {MAX_CLASS_FILES})")]
    TooManyClasses(usize),
    #[error("total decompressed class data exceeds the {MAX_TOTAL_CLASS_BYTES}-byte limit")]
    TotalClassBytesTooLarge,
    #[error("{0} is missing from the package")]
    MissingManifest(&'static str),
    #[error("{0} exceeds the {MAX_MANIFEST_BYTES}-byte manifest limit")]
    ManifestTooLarge(&'static str),
    #[error("manifest has an unsupported schema version: {0}")]
    UnsupportedSchema(u32),
    #[error("invalid mod id: {0}")]
    InvalidId(String),
    #[error("unsafe or invalid module path: {0}")]
    InvalidModulePath(String),
    #[error("failed to read manifest: {0}")]
    Read(#[from] std::io::Error),
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct FabricMetadata {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u32,
    pub id: String,
    pub version: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub environment: Option<String>,
    #[serde(default)]
    pub entrypoints: BTreeMap<String, Vec<serde_json::Value>>,
    #[serde(default)]
    pub depends: BTreeMap<String, String>,
    #[serde(default)]
    pub breaks: BTreeMap<String, String>,
    #[serde(default)]
    pub recommends: BTreeMap<String, String>,
    #[serde(default)]
    pub suggests: BTreeMap<String, String>,
    #[serde(default)]
    pub conflicts: BTreeMap<String, String>,
    #[serde(default)]
    pub provides: Vec<String>,
    #[serde(default)]
    pub jars: Vec<NestedJar>,
    #[serde(default)]
    pub mixins: Vec<serde_json::Value>,
    #[serde(rename = "accessWidener")]
    pub access_widener: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct NestedJar {
    pub file: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct PommeMetadata {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u32,
    pub id: String,
    pub version: String,
    pub name: Option<String>,
    /// Relative path to a WebAssembly module within the package.
    pub module: String,
    #[serde(default)]
    pub permissions: Vec<String>,
    /// Optional execution budget. The runtime will apply a stricter platform
    /// cap.
    pub fuel: Option<u64>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PackageKind {
    NativeWasm(PommeMetadata),
    FabricJava(Box<FabricMetadata>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct PackageReport {
    pub kind: PackageKind,
    pub archive_entries: usize,
    pub class_files: usize,
    pub wasm_files: usize,
    pub has_mixin_configs: bool,
    pub nested_jar_count: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Compatibility {
    /// A native Pomme WebAssembly package that can be handed to the sandbox
    /// runtime.
    NativeWasm,
    /// A Fabric archive with metadata but no Java classes or Java entrypoints.
    FabricMetadataOnly,
    /// Contains JVM bytecode or Java entrypoints; it needs translation or a
    /// JVM-compatible runtime.
    RequiresJavaCompatibility,
}

/// A conservative estimate of how much Java compatibility work a Fabric JAR
/// needs. This is a porting triage result, not a guarantee that a mod will
/// load.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompatibilityVerdict {
    NativeWasm,
    MetadataOnly,
    SimpleJavaCandidate,
    HardJava,
    Unsupported,
}

/// Classify a Fabric package using its metadata and bytecode inventory.
/// The simple candidate tier only means its observed bytecode fits the current
/// integer interpreter subset; it does not mean its Fabric entrypoint can run.
pub fn compatibility_verdict(
    package: &PackageReport,
    inventory: &JarCompatibilityReport,
) -> CompatibilityVerdict {
    let metadata = match &package.kind {
        PackageKind::NativeWasm(_) => return CompatibilityVerdict::NativeWasm,
        PackageKind::FabricJava(metadata) => metadata,
    };
    if package.class_files == 0
        && metadata.entrypoints.is_empty()
        && metadata.mixins.is_empty()
        && metadata.access_widener.is_none()
        && metadata.jars.is_empty()
    {
        return CompatibilityVerdict::MetadataOnly;
    }
    if inventory.skipped_classes > 0
        || !inventory.unsupported_or_high_risk_apis.is_empty()
        || inventory.feature_flags.contains("native_methods")
        || inventory.class_versions.keys().any(|version| *version > 65)
    {
        return CompatibilityVerdict::Unsupported;
    }
    let needs_fabric_runtime = !metadata.entrypoints.is_empty()
        || !metadata.mixins.is_empty()
        || metadata.access_widener.is_some()
        || !metadata.jars.is_empty()
        || package.has_mixin_configs;
    if needs_fabric_runtime
        || inventory.feature_flags.iter().any(|feature| {
            matches!(
                feature.as_str(),
                "static_initializers"
                    | "reflection_or_method_handles"
                    | "threading_or_concurrency"
                    | "filesystem_api_usage"
                    | "network_api_usage"
                    | "dynamic_class_loading"
            )
        })
    {
        return CompatibilityVerdict::HardJava;
    }
    const SUBSET: &[&str] = &[
        // Keep this list identical to the opcodes accepted by
        // java_subset::execute_static_i32. "nop" is intentionally omitted
        // because the interpreter rejects 0x00.
        "iconst_m1",
        "iconst_0",
        "iconst_1",
        "iconst_2",
        "iconst_3",
        "iconst_4",
        "iconst_5",
        "bipush",
        "sipush",
        "iload",
        "iload_0",
        "iload_1",
        "iload_2",
        "iload_3",
        "istore",
        "istore_0",
        "istore_1",
        "istore_2",
        "istore_3",
        "pop",
        "dup",
        "iadd",
        "isub",
        "imul",
        "idiv",
        "irem",
        "ineg",
        "ishl",
        "ishr",
        "iushr",
        "iand",
        "ior",
        "ixor",
        "iinc",
        "ifeq",
        "ifne",
        "iflt",
        "ifge",
        "ifgt",
        "ifle",
        "if_icmpeq",
        "if_icmpne",
        "if_icmplt",
        "if_icmpge",
        "if_icmpgt",
        "if_icmple",
        "goto",
        "ireturn",
    ];
    if !inventory.opcode_counts.is_empty()
        && inventory
            .opcode_counts
            .keys()
            .all(|opcode| SUBSET.contains(&opcode.as_str()))
        && inventory.invoked_methods.is_empty()
    {
        CompatibilityVerdict::SimpleJavaCandidate
    } else {
        CompatibilityVerdict::HardJava
    }
}

impl PackageReport {
    pub fn compatibility(&self) -> Compatibility {
        match &self.kind {
            PackageKind::NativeWasm(_) => Compatibility::NativeWasm,
            PackageKind::FabricJava(metadata)
                if self.class_files == 0
                    && metadata.entrypoints.is_empty()
                    && metadata.mixins.is_empty()
                    && metadata.access_widener.is_none()
                    && metadata.jars.is_empty() =>
            {
                Compatibility::FabricMetadataOnly
            }
            PackageKind::FabricJava(_) => Compatibility::RequiresJavaCompatibility,
        }
    }
}

pub fn inspect_package(bytes: &[u8]) -> Result<PackageReport, ModError> {
    let mut archive = ZipArchive::new(Cursor::new(bytes))?;
    let entry_count = archive.len();
    if entry_count > MAX_ARCHIVE_ENTRIES {
        return Err(ModError::TooManyEntries(entry_count));
    }

    let class_files = archive
        .file_names()
        .filter_map(Result::ok)
        .filter(|name| name.ends_with(".class"))
        .count();
    let wasm_files = archive
        .file_names()
        .filter_map(Result::ok)
        .filter(|name| name.ends_with(".wasm"))
        .count();
    let has_mixin_configs = archive
        .file_names()
        .filter_map(Result::ok)
        .any(|name| name.ends_with(".mixins.json"));

    let kind = if archive
        .file_names()
        .filter_map(Result::ok)
        .any(|name| name == POMME_MANIFEST)
    {
        let metadata: PommeMetadata = read_manifest(&mut archive, POMME_MANIFEST)?;
        validate_schema(metadata.schema_version)?;
        validate_id(&metadata.id)?;
        validate_module_path(&metadata.module)?;
        if !archive
            .file_names()
            .filter_map(Result::ok)
            .any(|name| name == metadata.module)
        {
            return Err(ModError::InvalidModulePath(metadata.module));
        }
        PackageKind::NativeWasm(metadata)
    } else {
        let metadata: FabricMetadata = read_manifest(&mut archive, FABRIC_MANIFEST)?;
        validate_schema(metadata.schema_version)?;
        validate_id(&metadata.id)?;
        for nested in &metadata.jars {
            validate_module_path(&nested.file)?;
        }
        PackageKind::FabricJava(Box::new(metadata))
    };

    let nested_jar_count = match &kind {
        PackageKind::FabricJava(metadata) => metadata.jars.len(),
        PackageKind::NativeWasm(_) => 0,
    };

    Ok(PackageReport {
        kind,
        archive_entries: entry_count,
        class_files,
        wasm_files,
        has_mixin_configs,
        nested_jar_count,
    })
}

/// Analyze every Java class in a Fabric JAR and return a compatibility
/// inventory. Class entries are bounded individually; malformed classes are
/// counted as skipped.
pub fn analyze_fabric_jar(bytes: &[u8]) -> Result<JarCompatibilityReport, ModError> {
    let mut archive = ZipArchive::new(Cursor::new(bytes))?;
    if archive.len() > MAX_ARCHIVE_ENTRIES {
        return Err(ModError::TooManyEntries(archive.len()));
    }
    let mut classes = Vec::new();
    let mut total_class_bytes: u64 = 0;
    let mut skipped_oversized = 0usize;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let name = entry.name()?.into_owned();
        if !name.ends_with(".class") {
            continue;
        }
        let declared = entry.size();
        // Skip individual classes that exceed the per-class limit; count them
        // as skipped so the verdict stays conservative.
        if declared > MAX_SINGLE_CLASS_BYTES {
            skipped_oversized += 1;
            continue;
        }
        if classes.len() >= MAX_CLASS_FILES {
            return Err(ModError::TooManyClasses(
                classes.len() + skipped_oversized + 1,
            ));
        }
        total_class_bytes = total_class_bytes
            .checked_add(declared)
            .ok_or(ModError::TotalClassBytesTooLarge)?;
        if total_class_bytes > MAX_TOTAL_CLASS_BYTES {
            return Err(ModError::TotalClassBytesTooLarge);
        }
        let mut class_bytes = Vec::with_capacity(declared as usize);
        entry.read_to_end(&mut class_bytes)?;
        // Defend against ZIP size lies: actual bytes may exceed the declared size.
        if class_bytes.len() as u64 > MAX_SINGLE_CLASS_BYTES {
            skipped_oversized += 1;
            continue;
        }
        classes.push((name, class_bytes));
    }
    let mut report = analyze_jar_classes(classes);
    report.skipped_classes += skipped_oversized;
    Ok(report)
}

fn read_manifest<T: for<'de> Deserialize<'de>>(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    name: &'static str,
) -> Result<T, ModError> {
    let mut file = archive
        .by_name(name)
        .map_err(|_| ModError::MissingManifest(name))?;
    if file.size() > MAX_MANIFEST_BYTES {
        return Err(ModError::ManifestTooLarge(name));
    }
    let mut bytes = Vec::with_capacity(file.size() as usize);
    file.read_to_end(&mut bytes)?;
    Ok(serde_json::from_slice(&bytes)?)
}

fn validate_schema(version: u32) -> Result<(), ModError> {
    if version == 1 {
        Ok(())
    } else {
        Err(ModError::UnsupportedSchema(version))
    }
}

fn validate_id(id: &str) -> Result<(), ModError> {
    let valid = (2..=64).contains(&id.len())
        && id.as_bytes()[0].is_ascii_alphabetic()
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-');
    if valid {
        Ok(())
    } else {
        Err(ModError::InvalidId(id.to_owned()))
    }
}

fn validate_module_path(path: &str) -> Result<(), ModError> {
    let parsed = Path::new(path);
    let safe = !path.is_empty()
        && !parsed.is_absolute()
        && parsed
            .components()
            .all(|component| matches!(component, Component::Normal(_)));
    if safe {
        Ok(())
    } else {
        Err(ModError::InvalidModulePath(path.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use zip::ZipWriter;
    use zip::write::SimpleFileOptions;

    use super::*;

    fn package(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        for (name, bytes) in entries {
            writer
                .start_file(*name, SimpleFileOptions::default())
                .unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    #[test]
    fn reads_fabric_metadata_and_detects_jvm_bytecode() {
        let manifest = br#"{
            "schemaVersion": 1,
            "id": "sample_mod",
            "version": "1.2.3",
            "entrypoints": { "client": ["sample.ClientInit"] },
            "depends": { "minecraft": ">=1.21" },
            "mixins": ["sample.mixins.json"],
            "jars": [{ "file": "META-INF/jars/library.jar" }]
        }"#;
        let bytes = package(&[
            (FABRIC_MANIFEST, manifest),
            ("sample/ClientInit.class", &[0xca, 0xfe, 0xba, 0xbe]),
            ("sample.mixins.json", b"{}"),
        ]);

        let report = inspect_package(&bytes).unwrap();
        assert_eq!(
            report.compatibility(),
            Compatibility::RequiresJavaCompatibility
        );
        assert_eq!(report.class_files, 1);
        assert!(report.has_mixin_configs);
        assert_eq!(report.nested_jar_count, 1);
        let PackageKind::FabricJava(metadata) = report.kind else {
            panic!("expected Fabric metadata");
        };
        assert_eq!(metadata.id, "sample_mod");
        assert_eq!(metadata.depends.get("minecraft").unwrap(), ">=1.21");
    }

    #[test]
    fn compatibility_verdict_marks_metadata_only_fabric_packages() {
        let manifest = br#"{"schemaVersion":1,"id":"metadata_only","version":"1"}"#;
        let bytes = package(&[(FABRIC_MANIFEST, manifest)]);
        let package_report = inspect_package(&bytes).unwrap();
        assert_eq!(
            compatibility_verdict(&package_report, &JarCompatibilityReport::default()),
            CompatibilityVerdict::MetadataOnly
        );
    }

    #[test]
    fn compatibility_verdict_only_calls_subset_code_a_candidate() {
        let manifest = br#"{"schemaVersion":1,"id":"pure_logic","version":"1"}"#;
        let bytes = package(&[(FABRIC_MANIFEST, manifest), ("Pure.class", b"class")]);
        let package_report = inspect_package(&bytes).unwrap();
        let inventory = JarCompatibilityReport {
            analyzed_classes: 1,
            class_versions: BTreeMap::from([(52, 1)]),
            opcode_counts: BTreeMap::from([
                ("iload_0".into(), 1),
                ("iadd".into(), 1),
                ("ireturn".into(), 1),
            ]),
            ..JarCompatibilityReport::default()
        };
        assert_eq!(
            compatibility_verdict(&package_report, &inventory),
            CompatibilityVerdict::SimpleJavaCandidate
        );
    }

    #[test]
    fn compatibility_verdict_rejects_opcodes_outside_interpreter_subset() {
        // "nop" is not implemented by the integer subset interpreter, so an
        // inventory that only contains it must not be marked SimpleJavaCandidate.
        let manifest = br#"{"schemaVersion":1,"id":"nop_only","version":"1"}"#;
        let bytes = package(&[(FABRIC_MANIFEST, manifest), ("Nop.class", b"class")]);
        let package_report = inspect_package(&bytes).unwrap();
        let inventory = JarCompatibilityReport {
            analyzed_classes: 1,
            class_versions: BTreeMap::from([(52, 1)]),
            opcode_counts: BTreeMap::from([("nop".into(), 1)]),
            ..JarCompatibilityReport::default()
        };
        assert_eq!(
            compatibility_verdict(&package_report, &inventory),
            CompatibilityVerdict::HardJava
        );
    }

    #[test]
    fn compatibility_verdict_marks_fabric_entrypoints_as_hard_java() {
        let manifest = br#"{"schemaVersion":1,"id":"entrypoint_mod","version":"1","entrypoints":{"main":["Example"]}}"#;
        let bytes = package(&[(FABRIC_MANIFEST, manifest), ("Example.class", b"class")]);
        let package_report = inspect_package(&bytes).unwrap();
        let inventory = JarCompatibilityReport {
            analyzed_classes: 1,
            class_versions: BTreeMap::from([(52, 1)]),
            opcode_counts: BTreeMap::from([("iload_0".into(), 1)]),
            ..JarCompatibilityReport::default()
        };
        assert_eq!(
            compatibility_verdict(&package_report, &inventory),
            CompatibilityVerdict::HardJava
        );
    }

    #[test]
    fn compatibility_verdict_rejects_unknown_classes_and_native_methods() {
        let manifest = br#"{"schemaVersion":1,"id":"risky_mod","version":"1"}"#;
        let bytes = package(&[(FABRIC_MANIFEST, manifest), ("Risky.class", b"class")]);
        let package_report = inspect_package(&bytes).unwrap();
        let mut inventory = JarCompatibilityReport {
            skipped_classes: 1,
            ..JarCompatibilityReport::default()
        };
        assert_eq!(
            compatibility_verdict(&package_report, &inventory),
            CompatibilityVerdict::Unsupported
        );
        inventory.skipped_classes = 0;
        inventory.feature_flags.insert("native_methods".into());
        assert_eq!(
            compatibility_verdict(&package_report, &inventory),
            CompatibilityVerdict::Unsupported
        );
    }

    #[test]
    fn analyze_fabric_jar_analyzes_valid_classes_under_budget() {
        use std::io::Write;

        use zip::write::SimpleFileOptions;
        let tiny = classfile::tests::simple_arithmetic_class();
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut cursor);
            let opts = SimpleFileOptions::default();
            zip.start_file(FABRIC_MANIFEST, opts).unwrap();
            zip.write_all(br#"{"schemaVersion":1,"id":"budget_ok","version":"1"}"#)
                .unwrap();
            zip.start_file("Tiny.class", opts).unwrap();
            zip.write_all(&tiny).unwrap();
            zip.finish().unwrap();
        }
        let bytes = cursor.into_inner();
        let report = analyze_fabric_jar(&bytes).unwrap();
        assert_eq!(report.analyzed_classes, 1);
        assert_eq!(report.skipped_classes, 0);
        // Sanity: budgets are ordered so a single class cannot exceed the total.
        const {
            assert!(MAX_TOTAL_CLASS_BYTES >= MAX_SINGLE_CLASS_BYTES);
            assert!(MAX_CLASS_FILES < MAX_ARCHIVE_ENTRIES);
        }
    }

    #[test]
    fn accepts_native_pomme_wasm_package() {
        let manifest = br#"{
            "schemaVersion": 1,
            "id": "native_example",
            "version": "0.1.0",
            "module": "modules/example.wasm",
            "permissions": ["events.read"],
            "fuel": 50000
        }"#;
        let bytes = package(&[
            (POMME_MANIFEST, manifest),
            ("modules/example.wasm", b"\\0asm"),
        ]);
        let report = inspect_package(&bytes).unwrap();
        assert_eq!(report.compatibility(), Compatibility::NativeWasm);
        assert_eq!(report.wasm_files, 1);
    }

    #[test]
    fn rejects_path_traversal_in_native_manifest() {
        let manifest = br#"{
            "schemaVersion": 1,
            "id": "native_example",
            "version": "0.1.0",
            "module": "../escape.wasm"
        }"#;
        let bytes = package(&[(POMME_MANIFEST, manifest)]);
        assert!(matches!(
            inspect_package(&bytes),
            Err(ModError::InvalidModulePath(_))
        ));
    }

    #[test]
    fn rejects_invalid_fabric_ids() {
        let manifest = br#"{"schemaVersion":1,"id":"bad id","version":"1"}"#;
        let bytes = package(&[(FABRIC_MANIFEST, manifest)]);
        assert!(matches!(
            inspect_package(&bytes),
            Err(ModError::InvalidId(_))
        ));
    }
}
