//! General-purpose Java execution through the embedded Ristretto JVM.
//!
//! This backend deliberately runs in interpreter mode on iOS. The upstream JIT
//! requires executable-memory support that is not universally available to
//! sideloaded iOS applications. The Java runtime image is supplied by Pomme;
//! it is never downloaded at launch time.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ristretto_vm::{ClassPath, ClassPathEntry, ConfigurationBuilder, Memory, VM, Value};
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};
use serde::Deserialize;
use thiserror::Error;

/// Errors returned by the general-purpose Java runtime.
#[derive(Debug, Error)]
pub enum RistrettoBackendError {
    #[error("Java runtime home is incomplete: {0}")]
    InvalidJavaHome(PathBuf),
    #[error("Ristretto JVM failed: {0}")]
    Runtime(#[from] ristretto_vm::Error),
    #[error("Java exception: {0}")]
    JavaException(String),
    #[error("Could not register Pomme's JVM bootstrap class: {0}")]
    BootstrapClass(String),
    #[error("Fabric runtime is incomplete: {0}")]
    InvalidFabricRuntime(PathBuf),
    #[error("Could not inspect the bundled Fabric runtime: {0}")]
    FabricRuntimeIo(#[from] std::io::Error),
    #[error("Invalid Java runtime JAR: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("Invalid Fabric runtime manifest: {0}")]
    FabricManifest(String),
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FabricRuntimeManifest {
    minecraft_version: String,
    java_major: u8,
    loader_jar: String,
    intermediary_jar: String,
    mixin_jar: String,
    provider_jar: String,
    #[serde(default)]
    minecraft_libraries: Vec<String>,
    builtin_mods: Vec<String>,
}

fn read_fabric_runtime_manifest(
    fabric_runtime: &Path,
) -> Result<FabricRuntimeManifest, RistrettoBackendError> {
    let manifest_path = fabric_runtime.join("runtime.json");
    let contents = std::fs::read_to_string(&manifest_path)?;
    serde_json::from_str(&contents)
        .map_err(|error| RistrettoBackendError::FabricManifest(error.to_string()))
}

/// A live Fabric/Knot JVM retained by Pomme so Java callbacks can be invoked
/// from the native game loop after mod initialization has completed.
#[derive(Clone)]
pub struct FabricSession {
    vm: Arc<VM>,
}

impl FabricSession {
    /// Invoke a static no-argument void method in the running mod runtime.
    pub async fn invoke_static_void(
        &self,
        class_name: &str,
        method_name: &str,
    ) -> Result<(), RistrettoBackendError> {
        let descriptor = format!("{method_name}()V");
        self.vm
            .invoke(class_name, descriptor, &[] as &[Value])
            .await?;
        Ok(())
    }

    /// Invoke a static no-argument method returning an integer.
    pub async fn invoke_static_i32(
        &self,
        class_name: &str,
        method_name: &str,
    ) -> Result<i32, RistrettoBackendError> {
        let descriptor = format!("{method_name}()I");
        match self
            .vm
            .invoke(class_name, descriptor, &[] as &[Value])
            .await?
        {
            Some(Value::Int(value)) => Ok(value),
            other => Err(RistrettoBackendError::BootstrapClass(format!(
                "{class_name}.{method_name} returned {other:?}, expected int"
            ))),
        }
    }

    /// Read a Java system property shared across the JVM's class loaders.
    pub fn system_property(&self, key: &str) -> Option<String> {
        self.vm.system_properties().get(key).cloned()
    }

    /// Update a bridge property visible to both Knot's target loader and the
    /// native-side JVM control thread.
    pub async fn set_system_property(
        &self,
        key: &str,
        value: &str,
    ) -> Result<(), RistrettoBackendError> {
        let arguments = [key, value];
        self.vm
            .invoke(
                "java.lang.System",
                "setProperty(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
                &arguments,
            )
            .await?;
        Ok(())
    }
}

fn java_major_from_home(java_home: &Path) -> Option<u8> {
    let release = std::fs::read_to_string(java_home.join("release")).ok()?;
    let version = release
        .lines()
        .find_map(|line| line.strip_prefix("JAVA_VERSION="))?
        .trim_matches('"');
    let mut components = version.split('.');
    let first = components.next()?.parse::<u8>().ok()?;
    if first == 1 {
        components.next()?.parse::<u8>().ok()
    } else {
        Some(first)
    }
}

fn validate_java_home(java_home: &Path) -> Result<(), RistrettoBackendError> {
    let modern_runtime = java_home.join("lib").join("modules").is_file();
    let legacy_runtime = java_home.join("jre").join("lib").join("rt.jar").is_file();
    if !java_home.join("release").is_file() && !java_home.join("version.txt").is_file() {
        return Err(RistrettoBackendError::InvalidJavaHome(
            java_home.to_path_buf(),
        ));
    }
    if !modern_runtime && !legacy_runtime {
        return Err(RistrettoBackendError::InvalidJavaHome(
            java_home.to_path_buf(),
        ));
    }
    Ok(())
}

/// Locate the Java runtime bundled at the root of the iOS app bundle.
pub fn bundled_java_home(major_version: u16) -> Option<PathBuf> {
    let bundle = std::env::current_exe().ok()?.parent()?.to_path_buf();
    let java_home = bundle.join("JavaRuntime").join(major_version.to_string());
    validate_java_home(&java_home).ok()?;
    Some(java_home)
}

fn throwable_string_field(object: &ristretto_classloader::Object, name: &str) -> Option<String> {
    object.value(name).ok()?.as_string().ok()
}

fn throwable_int_field(object: &ristretto_classloader::Object, name: &str) -> Option<i32> {
    object.value(name).ok()?.as_i32().ok()
}

/// Preserve the complete Java cause chain and stack frames instead of exposing
/// only the Ristretto Value debug label.
fn format_java_throwable(throwable: &Value) -> String {
    let mut current = throwable.clone();
    let mut output = String::new();
    let mut seen = std::collections::HashSet::new();

    for depth in 0..8 {
        let cause = {
            let Ok(object) = current.as_object_ref() else {
                if output.is_empty() {
                    return format!("{current:?}");
                }
                break;
            };
            let pointer = (&*object as *const ristretto_classloader::Object) as usize;
            if !seen.insert(pointer) {
                break;
            }

            let class_name = object.class().name().replace('/', ".");
            let message = throwable_string_field(&object, "detailMessage")
                .filter(|message| !message.is_empty());
            if depth == 0 {
                output.push_str(&class_name);
                if let Some(message) = message {
                    output.push_str(&format!(": {message}"));
                }
            } else {
                output.push_str(&format!("\\nCaused by: {class_name}"));
                if let Some(message) = message {
                    output.push_str(&format!(": {message}"));
                }
            }

            if let Ok(stack_trace) = object.value("stackTrace")
                && let Ok((_, frames)) = stack_trace.as_class_vec_ref()
            {
                for frame in frames.iter().take(24) {
                    let Ok(frame) = frame.as_object_ref() else {
                        continue;
                    };
                    let class = throwable_string_field(&frame, "declaringClass")
                        .unwrap_or_else(|| "<unknown>".to_string());
                    let method = throwable_string_field(&frame, "methodName")
                        .unwrap_or_else(|| "<unknown>".to_string());
                    let file = throwable_string_field(&frame, "fileName")
                        .unwrap_or_else(|| "Unknown Source".to_string());
                    let line = throwable_int_field(&frame, "lineNumber")
                        .filter(|line| *line >= 0)
                        .map(|line| format!(":{line}"))
                        .unwrap_or_default();
                    output.push_str(&format!("\\n    at {class}.{method}({file}{line})"));
                }
            }

            for field in ["file", "other", "reason"] {
                if let Some(value) = throwable_string_field(&object, field) {
                    output.push_str(&format!("\\n    {field}: {value}"));
                }
            }

            object
                .value("cause")
                .ok()
                .filter(|value| !value.is_null())
                .or_else(|| object.value("target").ok().filter(|value| !value.is_null()))
                .or_else(|| {
                    object
                        .value("exception")
                        .ok()
                        .filter(|value| !value.is_null())
                })
        };
        let Some(cause) = cause else {
            break;
        };
        current = cause;
    }

    output
}

async fn run_main_with_class_path(
    java_home: &Path,
    class_path: ClassPath,
    main_class: &str,
    arguments: &[&str],
) -> Result<Arc<VM>, RistrettoBackendError> {
    let configuration = ConfigurationBuilder::new()
        .java_home(java_home.to_path_buf())
        .class_path(class_path)
        .main_class(main_class)
        .interpreted(true)
        .batch_compilation(false)
        .build()?;

    let vm = VM::new(configuration).await?;
    match vm.invoke_main(arguments).await {
        Ok(_) => Ok(vm),
        Err(ristretto_vm::Error::Throwable(throwable)) => Err(
            RistrettoBackendError::JavaException(format_java_throwable(&throwable)),
        ),
        Err(error) => Err(RistrettoBackendError::Runtime(error)),
    }
}

/// Execute a Java class's static `main(String[])` method.
///
/// `java_home` must point to a Java 8+ runtime image containing `release` and
/// `lib/modules` (or the corresponding Java 8 runtime layout). `class_path`
/// may contain directories and JARs; include Fabric Loader, Fabric API,
/// dependency mods, and the target mod in loader order.
pub async fn run_java_main(
    java_home: &Path,
    class_path: &[PathBuf],
    main_class: &str,
    arguments: &[&str],
) -> Result<(), RistrettoBackendError> {
    validate_java_home(java_home)?;
    let _vm = run_main_with_class_path(
        java_home,
        ClassPath::from(class_path),
        main_class,
        arguments,
    )
    .await?;
    Ok(())
}

/// Invoke Fabric-style no-argument initializer methods from a shared JVM.
///
/// Each entry is a fully qualified class name and method name, such as
/// `("com.example.ExampleMod", "onInitialize")`. All supplied JARs share one
/// JVM/class-loader session so mod classes can resolve one another. Minimal
/// Fabric initializer interfaces are supplied for linkage; the complete Fabric
/// Loader lifecycle, Mixin transformation, Fabric API, and Minecraft bindings
/// are separate integration work.
pub async fn run_fabric_entrypoints(
    java_home: &Path,
    class_path: &[PathBuf],
    entrypoints: &[(String, String)],
) -> Result<(), RistrettoBackendError> {
    validate_java_home(java_home)?;
    if entrypoints.is_empty() {
        return Ok(());
    }

    let memory = Memory::new("pomme-fabric-bootstrap");
    memory
        .add_class(include_bytes!(
            "../resources/PommeFabricEntrypointRunner.class"
        ))
        .await
        .map_err(|error| RistrettoBackendError::BootstrapClass(error.to_string()))?;
    let fabric_api_classes: [&[u8]; 3] = [
        include_bytes!("../resources/net/fabricmc/api/ModInitializer.class").as_slice(),
        include_bytes!("../resources/net/fabricmc/api/ClientModInitializer.class").as_slice(),
        include_bytes!("../resources/net/fabricmc/api/PreLaunchEntrypoint.class").as_slice(),
    ];
    for class_bytes in fabric_api_classes {
        memory
            .add_class(class_bytes)
            .await
            .map_err(|error| RistrettoBackendError::BootstrapClass(error.to_string()))?;
    }

    let mut entries = vec![ClassPathEntry::Memory(memory)];
    entries.extend(ClassPath::from(class_path));
    let class_path = ClassPath::new(entries);

    let mut owned_arguments = Vec::with_capacity(entrypoints.len() * 2);
    for (class_name, method_name) in entrypoints {
        owned_arguments.push(class_name.as_str());
        owned_arguments.push(method_name.as_str());
    }

    let _vm = run_main_with_class_path(
        java_home,
        class_path,
        "PommeFabricEntrypointRunner",
        &owned_arguments,
    )
    .await?;
    Ok(())
}

/// Locate the official Fabric Loader runtime and Pomme GameProvider bundled
/// inside the iOS application.
pub fn bundled_fabric_runtime(minecraft_version: &str) -> Option<PathBuf> {
    let bundle = std::env::current_exe().ok()?.parent()?.to_path_buf();
    let runtime = bundle.join("FabricRuntime").join(minecraft_version);
    let manifest = read_fabric_runtime_manifest(&runtime).ok()?;
    if manifest.minecraft_version != minecraft_version {
        return None;
    }
    let lib = runtime.join("lib");
    let required = [
        manifest.loader_jar,
        manifest.intermediary_jar,
        manifest.mixin_jar,
        manifest.provider_jar,
    ];
    if required.iter().all(|name| lib.join(name).is_file())
        && manifest
            .minecraft_libraries
            .iter()
            .all(|name| lib.join(name).is_file())
        && manifest
            .builtin_mods
            .iter()
            .all(|name| runtime.join("builtin-mods").join(name).is_file())
    {
        Some(runtime)
    } else {
        None
    }
}

/// Flatten a multi-release JAR to the class/resource view for the active Java
/// version. Ristretto's upstream class-path reader currently reads only root
/// entries, while Java 9+ must select the highest META-INF/versions/N entry
/// when the manifest declares Multi-Release: true.
fn flatten_multi_release_jars(
    jars: &[PathBuf],
    game_dir: &Path,
    java_major: u8,
) -> Result<Vec<PathBuf>, RistrettoBackendError> {
    let staging_dir = game_dir.join(format!(".pomme-runtime-jars-java{java_major}"));
    if staging_dir.exists() {
        std::fs::remove_dir_all(&staging_dir)?;
    }
    std::fs::create_dir_all(&staging_dir)?;

    let mut staged = Vec::with_capacity(jars.len());
    let mut flattened_count = 0usize;
    for (index, jar_path) in jars.iter().enumerate() {
        let mut archive = ZipArchive::new(File::open(jar_path)?)?;
        let mut manifest_text = String::new();
        let is_multi_release = match archive.by_name("META-INF/MANIFEST.MF") {
            Ok(mut manifest) => {
                manifest.read_to_string(&mut manifest_text)?;
                let mut unfolded = Vec::<String>::new();
                for line in manifest_text.lines() {
                    if line.starts_with(' ') {
                        if let Some(previous) = unfolded.last_mut() {
                            previous.push_str(&line[1..]);
                        }
                    } else {
                        unfolded.push(line.to_owned());
                    }
                }
                unfolded.iter().any(|line| {
                    line.split_once(':').is_some_and(|(key, value)| {
                        key.eq_ignore_ascii_case("Multi-Release")
                            && value.trim().eq_ignore_ascii_case("true")
                    })
                })
            }
            Err(zip::result::ZipError::FileNotFound) => false,
            Err(error) => return Err(error.into()),
        };
        if !is_multi_release {
            staged.push(jar_path.clone());
            continue;
        }

        let mut base_entries = BTreeMap::<String, Vec<u8>>::new();
        let mut versioned_entries = BTreeMap::<String, (u8, Vec<u8>)>::new();
        for entry_index in 0..archive.len() {
            let mut entry = archive.by_index(entry_index)?;
            if entry.is_dir() {
                continue;
            }
            let name = entry.name().to_owned();
            let mut bytes = Vec::with_capacity(usize::try_from(entry.size()).unwrap_or(0));
            entry.read_to_end(&mut bytes)?;

            if let Some(versioned) = name.strip_prefix("META-INF/versions/") {
                let Some((version, logical_name)) = versioned.split_once('/') else {
                    continue;
                };
                let Ok(version) = version.parse::<u8>() else {
                    continue;
                };
                // JEP 238 never versions META-INF entries themselves.
                if version < 9
                    || version > java_major
                    || logical_name.is_empty()
                    || logical_name.starts_with("META-INF/")
                {
                    continue;
                }
                let replace = versioned_entries
                    .get(logical_name)
                    .is_none_or(|(current_version, _)| version > *current_version);
                if replace {
                    versioned_entries.insert(logical_name.to_owned(), (version, bytes));
                }
            } else {
                base_entries.insert(name, bytes);
            }
        }

        if versioned_entries.is_empty() {
            staged.push(jar_path.clone());
            continue;
        }
        for (name, (_, bytes)) in versioned_entries {
            base_entries.insert(name, bytes);
        }

        let file_name = jar_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("runtime.jar");
        let output_path = staging_dir.join(format!("{index:04}-{file_name}"));
        let output_file = File::create(&output_path)?;
        let mut writer = ZipWriter::new(output_file);
        for (name, bytes) in base_entries {
            writer.start_file(name, SimpleFileOptions::default())?;
            writer.write_all(&bytes)?;
        }
        writer.finish()?;
        staged.push(output_path);
        flattened_count += 1;
    }

    if std::env::var_os("POMME_FABRIC_DEBUG").is_some() {
        eprintln!(
            "[POMME_MRJAR] flattened {flattened_count} multi-release JAR(s) for Java {java_major}"
        );
    }
    Ok(staged)
}

/// Start the real Fabric Loader/Knot client bootstrap against Pomme's native
/// game directory. Fabric Loader performs mod discovery, dependency solving,
/// language-adapter resolution, Mixin bootstrap and entrypoint lifecycle.
pub async fn run_fabric_loader(
    java_home: &Path,
    fabric_runtime: &Path,
    game_dir: &Path,
    mods_dir: &Path,
    minecraft_version: &str,
) -> Result<FabricSession, RistrettoBackendError> {
    validate_java_home(java_home)?;
    std::fs::create_dir_all(game_dir)?;
    std::fs::create_dir_all(mods_dir)?;
    // Fabric Loader stores extracted nested mods in this directory. Create it
    // natively instead of relying on the embedded JDK Files.createDirectories
    // implementation.
    std::fs::create_dir_all(game_dir.join(".fabric"))?;
    let manifest = read_fabric_runtime_manifest(fabric_runtime)?;
    if java_major_from_home(java_home) != Some(manifest.java_major) {
        return Err(RistrettoBackendError::InvalidJavaHome(
            java_home.to_path_buf(),
        ));
    }
    if manifest.minecraft_version != minecraft_version {
        return Err(RistrettoBackendError::InvalidFabricRuntime(
            fabric_runtime.to_path_buf(),
        ));
    }
    let lib_dir = fabric_runtime.join("lib");
    let mut jars = std::fs::read_dir(&lib_dir)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            let is_jar = path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("jar"));
            let is_unpatched_loader =
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| {
                        name.starts_with("fabric-loader-") && !name.ends_with("-pomme.jar")
                    });
            is_jar && !is_unpatched_loader
        })
        .collect::<Vec<_>>();
    jars.sort();
    let required = [
        &manifest.loader_jar,
        &manifest.intermediary_jar,
        &manifest.mixin_jar,
        &manifest.provider_jar,
    ];
    if !required.iter().all(|name| lib_dir.join(name).is_file())
        || !manifest
            .minecraft_libraries
            .iter()
            .all(|name| lib_dir.join(name).is_file())
    {
        return Err(RistrettoBackendError::InvalidFabricRuntime(
            fabric_runtime.to_path_buf(),
        ));
    }

    let original_intermediary_jar = lib_dir.join(&manifest.intermediary_jar);
    let staged_jars = flatten_multi_release_jars(&jars, game_dir, manifest.java_major)?;
    let intermediary_jar = staged_jars
        .iter()
        .zip(&jars)
        .find(|(_, original)| *original == &original_intermediary_jar)
        .map(|(staged, _)| staged.clone())
        .unwrap_or(original_intermediary_jar);
    let class_path = ClassPath::from(&staged_jars);
    let game_dir = game_dir.to_string_lossy();
    let mods_dir = mods_dir.to_string_lossy();
    let intermediary_jar = intermediary_jar.to_string_lossy();
    let vm = run_main_with_class_path(
        java_home,
        class_path,
        "PommeFabricLauncher",
        &[
            game_dir.as_ref(),
            mods_dir.as_ref(),
            minecraft_version,
            intermediary_jar.as_ref(),
        ],
    )
    .await?;
    Ok(FabricSession { vm })
}

#[cfg(test)]
mod multi_release_jar_tests {
    use super::*;
        use std::time::{SystemTime, UNIX_EPOCH};

    fn make_multi_release_jar(path: &Path) {
        let mut writer = ZipWriter::new(File::create(path).expect("create test JAR"));
        let options = SimpleFileOptions::default();
        writer
            .start_file("META-INF/MANIFEST.MF", options)
            .expect("start manifest");
        writer
            .write_all(b"Manifest-Version: 1.0\r\nMulti-Release: true\r\n\r\n")
            .expect("write manifest");
        writer
            .start_file("example/Version.class", options)
            .expect("start base class");
        writer.write_all(b"base").expect("write base class");
        writer
            .start_file("META-INF/versions/9/example/Version.class", options)
            .expect("start Java 9 class");
        writer.write_all(b"java9").expect("write Java 9 class");
        writer
            .start_file("META-INF/versions/21/example/Version.class", options)
            .expect("start Java 21 class");
        writer.write_all(b"java21").expect("write Java 21 class");
        writer
            .start_file("META-INF/versions/21/META-INF/services/example.Service", options)
            .expect("start non-versionable META-INF resource");
        writer.write_all(b"must-not-override").expect("write service");
        writer.finish().expect("finish test JAR");
    }

    fn read_entry(path: &Path, name: &str) -> Vec<u8> {
        let mut archive = ZipArchive::new(File::open(path).expect("open staged JAR"))
            .expect("read staged JAR");
        let mut entry = archive.by_name(name).expect("find staged entry");
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).expect("read staged entry");
        bytes
    }

    #[test]
    fn selects_highest_multi_release_entry_not_newer_than_runtime() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "pomme-mrjar-test-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).expect("create test directory");
        let source = root.join("fixture.jar");
        make_multi_release_jar(&source);

        let staged = flatten_multi_release_jars(&[source], &root, 17)
            .expect("flatten Java 17 class path");
        assert_eq!(staged.len(), 1);
        assert_eq!(
            read_entry(&staged[0], "example/Version.class"),
            b"java9"
        );
        assert!(
            ZipArchive::new(File::open(&staged[0]).expect("open staged JAR"))
                .expect("read staged JAR")
                .by_name("META-INF/versions/9/example/Version.class")
                .is_err(),
            "versioned paths must not remain after flattening"
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn selects_java_21_entry_and_does_not_version_meta_inf_resources() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "pomme-mrjar-test-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).expect("create test directory");
        let source = root.join("fixture.jar");
        make_multi_release_jar(&source);

        let staged = flatten_multi_release_jars(&[source], &root, 21)
            .expect("flatten Java 21 class path");
        assert_eq!(read_entry(&staged[0], "example/Version.class"), b"java21");
        assert!(
            ZipArchive::new(File::open(&staged[0]).expect("open staged JAR"))
                .expect("read staged JAR")
                .by_name("META-INF/services/example.Service")
                .is_err(),
            "META-INF entries under versions must not be promoted"
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn leaves_non_multi_release_jars_untouched() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "pomme-mrjar-test-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).expect("create test directory");
        let source = root.join("plain.jar");
        let mut writer = ZipWriter::new(File::create(&source).expect("create plain JAR"));
        writer
            .start_file("example/Plain.class", SimpleFileOptions::default())
            .expect("start plain class");
        writer.write_all(b"plain").expect("write plain class");
        writer.finish().expect("finish plain JAR");

        let staged = flatten_multi_release_jars(&[source.clone()], &root, 21)
            .expect("process plain class path");
        assert_eq!(staged, [source]);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn manifest_attribute_is_case_insensitive_and_unfolded() {
        let manifest = b"Manifest-Version: 1.0\r\nMulti-Release: tr\r\n ue\r\n\r\n";
        let text = String::from_utf8(manifest.to_vec()).expect("UTF-8 manifest");
        let mut unfolded = Vec::<String>::new();
        for line in text.lines() {
            if line.starts_with(' ') {
                if let Some(previous) = unfolded.last_mut() {
                    previous.push_str(&line[1..]);
                }
            } else {
                unfolded.push(line.to_owned());
            }
        }
        assert!(unfolded.iter().any(|line| {
            line.split_once(':').is_some_and(|(key, value)| {
                key.eq_ignore_ascii_case("Multi-Release")
                    && value.trim().eq_ignore_ascii_case("true")
            })
        }));
    }
}
