//! Experimental Java/Fabric mod discovery and dependency-aware bootstrap.
//! This is a compatibility subset, not a complete Fabric Loader.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use pomme_mod::{FabricMetadata, PackageKind};

/// Copy a user-selected Java mod into the active version's isolated mods
/// folder. Existing names are preserved by choosing a numbered suffix instead
/// of silently replacing a mod the user already installed.
pub fn import_jar_file(source: &Path, mods_dir: &Path) -> std::io::Result<PathBuf> {
    if !source
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("jar"))
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Only .jar files can be imported as Java mods",
        ));
    }
    std::fs::create_dir_all(mods_dir)?;
    let file_name = source.file_name().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Selected file has no name",
        )
    })?;
    let destination = mods_dir.join(file_name);
    let destination = if !destination.exists() {
        destination
    } else {
        let stem = source
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("mod");
        let extension = source
            .extension()
            .and_then(|name| name.to_str())
            .unwrap_or("jar");
        (2..=999)
            .map(|number| mods_dir.join(format!("{stem} ({number}).{extension}")))
            .find(|candidate| !candidate.exists())
            .ok_or_else(|| std::io::Error::other("Too many files with this mod name"))?
    };
    std::fs::copy(source, &destination)?;
    Ok(destination)
}

struct InstalledMod {
    path: PathBuf,
    bytes: Vec<u8>,
    metadata: FabricMetadata,
}

#[derive(Clone, Debug)]
struct ModDescriptor {
    id: String,
    version: String,
    provides: Vec<String>,
    depends: BTreeMap<String, String>,
    breaks: BTreeMap<String, String>,
    conflicts: BTreeMap<String, String>,
}

impl From<&FabricMetadata> for ModDescriptor {
    fn from(metadata: &FabricMetadata) -> Self {
        Self {
            id: metadata.id.clone(),
            version: metadata.version.clone(),
            provides: metadata.provides.clone(),
            depends: metadata.depends.clone(),
            breaks: metadata.breaks.clone(),
            conflicts: metadata.conflicts.clone(),
        }
    }
}

fn parse_fabric_version(value: &str) -> Option<semver::Version> {
    let normalized = match value.matches('.').count() {
        0 => format!("{value}.0.0"),
        1 => format!("{value}.0"),
        _ => value.to_owned(),
    };
    semver::Version::parse(&normalized).ok()
}

fn version_matches(version: &str, requirement: &str) -> bool {
    if requirement == "*" {
        return true;
    }
    let is_range = requirement.bytes().any(|byte| {
        matches!(
            byte,
            b'<' | b'>' | b'=' | b'^' | b'~' | b'*' | b'x' | b'X' | b','
        )
    });
    if !is_range {
        let short_minor_requirement = requirement.matches('.').count() == 1;
        return match (
            parse_fabric_version(version),
            parse_fabric_version(requirement),
        ) {
            (Some(version), Some(required)) if short_minor_requirement => {
                version.major == required.major && version.minor == required.minor
            }
            (Some(version), Some(required)) => version == required,
            _ => version == requirement,
        };
    }
    match (
        parse_fabric_version(version),
        semver::VersionReq::parse(requirement),
    ) {
        (Some(version), Ok(requirement)) => requirement.matches(&version),
        _ => version == requirement,
    }
}

/// Resolve required dependencies and produce a stable topological order.
/// Minecraft and Java are checked against the selected game/runtime versions;
/// fabricloader is recognized as a partially implemented platform service.
fn resolve_load_order(
    mods: &[ModDescriptor],
    minecraft_version: &str,
) -> (Vec<usize>, Vec<(usize, String)>) {
    let mut ids = BTreeMap::<String, usize>::new();
    let mut duplicate = BTreeSet::<usize>::new();
    for (index, item) in mods.iter().enumerate() {
        for name in std::iter::once(&item.id).chain(item.provides.iter()) {
            if let Some(previous) = ids.insert(name.clone(), index) {
                duplicate.insert(previous);
                duplicate.insert(index);
            }
        }
    }

    let mut blocked = vec![None::<String>; mods.len()];
    for index in duplicate {
        blocked[index] = Some("duplicate mod id or provided alias".into());
    }
    let mut dependencies = vec![Vec::<usize>::new(); mods.len()];

    for (index, item) in mods.iter().enumerate() {
        for (dependency, requirement) in &item.depends {
            let resolved = match dependency.as_str() {
                "minecraft" => {
                    if !version_matches(minecraft_version, requirement) {
                        blocked[index] = Some(format!(
                            "Minecraft {minecraft_version} does not satisfy {requirement}"
                        ));
                    }
                    None
                }
                "java" => {
                    if !version_matches("21.0.0", requirement) {
                        blocked[index] = Some(format!(
                            "Pomme's Java 21 compatibility tier does not satisfy {requirement}"
                        ));
                    }
                    None
                }
                // Pomme currently provides a small subset of Fabric APIs, not
                // a version-identical Fabric Loader implementation.
                "fabricloader" => None,
                _ => ids.get(dependency).copied(),
            };

            if !matches!(dependency.as_str(), "minecraft" | "java" | "fabricloader") {
                match resolved {
                    None => {
                        blocked[index].get_or_insert_with(|| {
                            format!("required mod dependency '{dependency}' is not installed")
                        });
                    }
                    Some(dep_index) => {
                        if !version_matches(&mods[dep_index].version, requirement) {
                            blocked[index].get_or_insert_with(|| {
                                format!(
                                    "dependency '{dependency}' version {} does not satisfy {requirement}",
                                    mods[dep_index].version
                                )
                            });
                        }
                        if dep_index != index {
                            dependencies[index].push(dep_index);
                        } else {
                            blocked[index].get_or_insert_with(|| {
                                format!("mod depends on itself through '{dependency}'")
                            });
                        }
                    }
                }
            }
        }

        for (conflict, requirement) in item.breaks.iter().chain(item.conflicts.iter()) {
            if ids.get(conflict).is_some_and(|other| {
                *other != index && version_matches(&mods[*other].version, requirement)
            }) {
                blocked[index].get_or_insert_with(|| {
                    format!("installed mod '{conflict}' is declared incompatible")
                });
            }
        }
        dependencies[index].sort_unstable();
        dependencies[index].dedup();
    }

    // If a dependency is already blocked, block its dependents as well.
    loop {
        let mut changed = false;
        for index in 0..mods.len() {
            if blocked[index].is_some() {
                continue;
            }
            if let Some(dependency) = dependencies[index]
                .iter()
                .find(|dependency| blocked[**dependency].is_some())
            {
                blocked[index] = Some(format!(
                    "required dependency '{}' cannot be loaded",
                    mods[*dependency].id
                ));
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    let mut indegree = vec![0usize; mods.len()];
    let mut dependents = vec![Vec::<usize>::new(); mods.len()];
    for index in 0..mods.len() {
        if blocked[index].is_some() {
            continue;
        }
        for dependency in &dependencies[index] {
            if blocked[*dependency].is_none() {
                indegree[index] += 1;
                dependents[*dependency].push(index);
            }
        }
    }

    let mut ready = BTreeSet::new();
    for index in 0..mods.len() {
        if blocked[index].is_none() && indegree[index] == 0 {
            ready.insert(index);
        }
    }
    let mut order = Vec::new();
    while let Some(index) = ready.pop_first() {
        order.push(index);
        for dependent in &dependents[index] {
            indegree[*dependent] -= 1;
            if indegree[*dependent] == 0 {
                ready.insert(*dependent);
            }
        }
    }

    let ordered: BTreeSet<usize> = order.iter().copied().collect();
    for index in 0..mods.len() {
        if blocked[index].is_none() && !ordered.contains(&index) {
            blocked[index] = Some("dependency cycle detected".into());
        }
    }
    loop {
        let mut changed = false;
        for index in 0..mods.len() {
            if blocked[index].is_some() {
                continue;
            }
            if let Some(dependency) = dependencies[index]
                .iter()
                .find(|dependency| blocked[**dependency].is_some())
            {
                blocked[index] = Some(format!(
                    "required dependency '{}' cannot be loaded",
                    mods[*dependency].id
                ));
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    order.retain(|index| blocked[*index].is_none());
    let reasons = blocked
        .into_iter()
        .enumerate()
        .filter_map(|(index, reason)| reason.map(|reason| (index, reason)))
        .collect();
    (order, reasons)
}

/// Inspect the selected version's mods folder, resolve dependencies, then
/// execute only entrypoints that fit the fuel-bounded JVM subset.
pub fn initialize_installed(mods_dir: &Path, minecraft_version: &str) {
    let entries = match std::fs::read_dir(mods_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => {
            tracing::warn!(
                "Could not read Java mods directory {}: {error}",
                mods_dir.display()
            );
            return;
        }
    };

    let mut jars: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("jar"))
        })
        .collect();
    jars.sort();

    if jars.len() > 512 {
        tracing::warn!("Too many Java mod JARs; considering the first 512");
        jars.truncate(512);
    }

    const MAX_INSTALLED_MODS: usize = 512;
    const MAX_INSTALLED_ARCHIVE_BYTES: u64 = 256 * 1024 * 1024;
    let mut installed = Vec::<InstalledMod>::new();
    let mut total_archive_bytes = 0u64;
    for path in jars {
        if installed.len() >= MAX_INSTALLED_MODS {
            tracing::warn!("Skipping remaining Java mods: installed-mod scan limit reached");
            break;
        }
        match std::fs::metadata(&path) {
            Ok(metadata)
                if metadata.len() <= 128 * 1024 * 1024
                    && total_archive_bytes.saturating_add(metadata.len())
                        <= MAX_INSTALLED_ARCHIVE_BYTES =>
            {
                total_archive_bytes += metadata.len();
            }
            Ok(metadata) => {
                tracing::warn!(
                    "Skipping Java mod {}: archive exceeds the per-JAR or total scan size limit ({} bytes)",
                    path.display(),
                    metadata.len()
                );
                continue;
            }
            Err(error) => {
                tracing::warn!("Could not inspect Java mod {}: {error}", path.display());
                continue;
            }
        }

        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) => {
                tracing::warn!("Could not read Java mod {}: {error}", path.display());
                continue;
            }
        };
        let package = match pomme_mod::inspect_package(&bytes) {
            Ok(package) => package,
            Err(error) => {
                tracing::warn!("Skipping invalid mod archive {}: {error}", path.display());
                continue;
            }
        };
        let PackageKind::FabricJava(metadata) = package.kind else {
            continue;
        };
        installed.push(InstalledMod {
            path,
            bytes,
            metadata: *metadata,
        });
    }

    let descriptors: Vec<ModDescriptor> = installed
        .iter()
        .map(|item| ModDescriptor::from(&item.metadata))
        .collect();
    let selected_mod_version = mods_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("unknown");
    let (order, blocked) = resolve_load_order(&descriptors, selected_mod_version);
    for (index, reason) in blocked {
        tracing::warn!(
            "Skipping Java mod {}: {reason}",
            installed[index].path.display()
        );
    }

    let mut loaded_ids = BTreeSet::<String>::new();
    let mut loaded_classpath_indices = Vec::<usize>::new();
    for index in order {
        let item = &installed[index];
        let missing_runtime_dependency = item.metadata.depends.keys().find(|dependency| {
            !matches!(dependency.as_str(), "minecraft" | "java" | "fabricloader")
                && !loaded_ids.contains(*dependency)
        });
        if let Some(dependency) = missing_runtime_dependency {
            tracing::warn!(
                "Skipping Java mod {}: dependency '{dependency}' did not initialize successfully",
                item.path.display()
            );
            continue;
        }

        let has_client_entrypoint = ["preLaunch", "main", "client"]
            .iter()
            .any(|group| item.metadata.entrypoints.contains_key(*group));
        if !has_client_entrypoint {
            match pomme_jvm::validate_fabric_mod_jar(&item.bytes) {
                Ok(class_count) => {
                    tracing::info!(
                        "Indexed Java library mod {} from {} ({} class(es))",
                        item.metadata.id,
                        item.path.display(),
                        class_count
                    );
                    loaded_ids.insert(item.metadata.id.clone());
                    loaded_ids.extend(item.metadata.provides.iter().cloned());
                    loaded_classpath_indices.push(index);
                }
                Err(error) => {
                    tracing::warn!(
                        "Could not index Java library mod {}: {error}",
                        item.path.display()
                    );
                }
            }
            continue;
        }

        let dependency_refs: Vec<&[u8]> = loaded_classpath_indices
            .iter()
            .map(|dependency_index| installed[*dependency_index].bytes.as_slice())
            .collect();
        match pomme_jvm::run_fabric_mod_jar_with_classpath_and_version(
            &item.bytes,
            &dependency_refs,
            250_000,
            minecraft_version,
        ) {
            Ok(report) => {
                tracing::info!(
                    "Loaded experimental Java mod {} from {} ({} class(es))",
                    report.mod_id,
                    item.path.display(),
                    report.initialized_classes.len()
                );
                for message in report.logs {
                    tracing::info!("[Java mod {}] {}", report.mod_id, message);
                }
                loaded_ids.insert(item.metadata.id.clone());
                loaded_ids.extend(item.metadata.provides.iter().cloned());
                loaded_classpath_indices.push(index);
            }
            Err(error) => {
                tracing::warn!(
                    "Java mod {} needs JVM/Fabric features not implemented yet: {error}",
                    item.path.display()
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_only_jar_files_and_keeps_duplicate_names() {
        let root = std::env::temp_dir().join(format!("pomme-mod-import-{}", std::process::id()));
        let source_dir = root.join("source");
        let mods_dir = root.join("mods");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&source_dir).unwrap();
        let source = source_dir.join("example.jar");
        std::fs::write(&source, b"first jar").unwrap();

        let first = import_jar_file(&source, &mods_dir).unwrap();
        let second = import_jar_file(&source, &mods_dir).unwrap();
        assert_eq!(first.file_name().unwrap(), "example.jar");
        assert_eq!(second.file_name().unwrap(), "example (2).jar");
        assert_eq!(std::fs::read(first).unwrap(), b"first jar");
        assert_eq!(std::fs::read(second).unwrap(), b"first jar");

        let invalid = source_dir.join("not-a-mod.zip");
        std::fs::write(&invalid, b"zip").unwrap();
        assert_eq!(
            import_jar_file(&invalid, &mods_dir).unwrap_err().kind(),
            std::io::ErrorKind::InvalidInput
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    fn descriptor(id: &str, version: &str, depends: &[(&str, &str)]) -> ModDescriptor {
        ModDescriptor {
            id: id.into(),
            version: version.into(),
            provides: Vec::new(),
            depends: depends
                .iter()
                .map(|(key, value)| ((*key).into(), (*value).into()))
                .collect(),
            breaks: BTreeMap::new(),
            conflicts: BTreeMap::new(),
        }
    }

    #[test]
    fn resolves_dependencies_before_dependents_even_when_files_sort_backwards() {
        let mods = vec![
            descriptor("addon", "1.0.0", &[("library", ">=2.0.0")]),
            descriptor("library", "2.1.0", &[]),
        ];
        let (order, blocked) = resolve_load_order(&mods, "1.21.4");
        assert!(blocked.is_empty());
        assert_eq!(order, [1, 0]);
    }

    #[test]
    fn blocks_missing_and_incompatible_required_dependencies() {
        let mods = vec![
            descriptor("needs_missing", "1.0.0", &[("absent", "*")]),
            descriptor("needs_newer", "1.0.0", &[("library", ">=2.0.0")]),
            descriptor("library", "1.4.0", &[]),
        ];
        let (order, blocked) = resolve_load_order(&mods, "1.21.4");
        assert_eq!(order, [2]);
        assert_eq!(blocked.len(), 2);
    }

    #[test]
    fn blocks_mods_that_require_another_minecraft_version() {
        let mods = vec![descriptor(
            "wrong_game",
            "1.0.0",
            &[("minecraft", ">=1.22")],
        )];
        let (order, blocked) = resolve_load_order(&mods, "1.21.4");
        assert!(order.is_empty());
        assert_eq!(blocked.len(), 1);
    }

    #[test]
    fn detects_dependency_cycles_and_propagates_the_failure() {
        let mods = vec![
            descriptor("a", "1.0.0", &[("b", "*")]),
            descriptor("b", "1.0.0", &[("a", "*")]),
            descriptor("c", "1.0.0", &[("a", "*")]),
        ];
        let (order, blocked) = resolve_load_order(&mods, "1.21.4");
        assert!(order.is_empty());
        assert_eq!(blocked.len(), 3);
    }

    #[test]
    fn rejects_duplicate_mod_ids_instead_of_choosing_an_arbitrary_jar() {
        let mods = vec![
            descriptor("duplicate", "1.0.0", &[]),
            descriptor("duplicate", "2.0.0", &[]),
        ];
        let (order, blocked) = resolve_load_order(&mods, "1.21.4");
        assert!(order.is_empty());
        assert_eq!(blocked.len(), 2);
    }

    #[test]
    fn only_blocks_a_declared_conflict_when_its_version_matches() {
        let mut incompatible = descriptor("incompatible", "1.0.0", &[]);
        incompatible
            .conflicts
            .insert("library".into(), ">=2.0.0".into());
        let mods = vec![incompatible, descriptor("library", "1.5.0", &[])];
        let (order, blocked) = resolve_load_order(&mods, "1.21.4");
        assert!(blocked.is_empty());
        assert_eq!(order, [0, 1]);
    }

    #[test]
    fn treats_plain_dependency_versions_as_exact_matches() {
        assert!(version_matches("1.20.1", "1.20.1"));
        assert!(!version_matches("1.20.2", "1.20.1"));
        assert!(version_matches("1.20.2", ">=1.20.1"));
    }

    #[test]
    fn supports_short_fabric_versions_and_platform_dependencies() {
        assert!(version_matches("1.21", ">=1.20.1"));
        assert!(version_matches("1.21.4", "*"));
        let mods = vec![descriptor(
            "platform_mod",
            "1.0.0",
            &[
                ("minecraft", "1.21"),
                ("fabricloader", ">=0.16.0"),
                ("java", ">=21"),
            ],
        )];
        let (order, blocked) = resolve_load_order(&mods, "1.21.4");
        assert!(blocked.is_empty());
        assert_eq!(order, [0]);
    }

    #[test]
    fn honors_provided_aliases_and_java_runtime_constraints() {
        let mut library = descriptor("library", "3.0.0", &[]);
        library.provides.push("library_api".into());
        let addon = descriptor(
            "addon",
            "1.0.0",
            &[("library_api", "^3.0.0"), ("java", ">=21")],
        );
        let (order, blocked) = resolve_load_order(&[addon, library], "1.21.4");
        assert!(blocked.is_empty());
        assert_eq!(order, [1, 0]);
    }
}
