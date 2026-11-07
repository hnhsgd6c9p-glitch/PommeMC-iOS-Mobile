//! Pomme's Rust-implemented JVM runtime.
//!
//! This crate starts with a fuel-bounded interpreter for a well-defined JVM
//! integer-bytecode subset. It is deliberately not advertised as a complete
//! JVM: class loading, verification, object semantics, exceptions, threads,
//! the Java class library, and dynamic linking are subsequent milestones.

use std::collections::{BTreeMap, BTreeSet};

use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReturnValue {
    Void,
    Int(i32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExecutionLimits {
    pub max_stack: usize,
    pub max_locals: usize,
    pub fuel: u64,
}

impl Default for ExecutionLimits {
    fn default() -> Self {
        Self {
            max_stack: 1024,
            max_locals: 256,
            fuel: 1_000_000,
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum VmError {
    #[error("unsupported opcode 0x{0:02x} at bytecode offset {1}")]
    UnsupportedOpcode(u8, usize),
    #[error("truncated instruction at bytecode offset {0}")]
    TruncatedInstruction(usize),
    #[error("invalid branch target {0}")]
    InvalidBranchTarget(isize),
    #[error("operand stack underflow")]
    StackUnderflow,
    #[error("operand stack exceeded configured limit")]
    StackOverflow,
    #[error("local variable index {0} is out of range")]
    InvalidLocal(usize),
    #[error("integer division by zero")]
    DivisionByZero,
    #[error("execution exhausted its fuel budget")]
    FuelExhausted,
    #[error("method completed without a return instruction")]
    MissingReturn,
    #[error("void return used with a non-empty operand stack")]
    InvalidReturnState,
}

#[derive(Debug)]
pub struct Vm {
    limits: ExecutionLimits,
}

impl Vm {
    pub fn new(limits: ExecutionLimits) -> Self {
        Self { limits }
    }

    pub fn execute_static_int(&self, code: &[u8], initial_locals: &[i32]) -> Result<i32, VmError> {
        match self.execute(code, initial_locals)? {
            ReturnValue::Int(value) => Ok(value),
            ReturnValue::Void => Err(VmError::MissingReturn),
        }
    }

    pub fn execute(&self, code: &[u8], initial_locals: &[i32]) -> Result<ReturnValue, VmError> {
        if initial_locals.len() > self.limits.max_locals {
            return Err(VmError::InvalidLocal(initial_locals.len() - 1));
        }
        let mut locals = vec![0i32; self.limits.max_locals];
        locals[..initial_locals.len()].copy_from_slice(initial_locals);
        let mut stack: Vec<i32> = Vec::with_capacity(self.limits.max_stack.min(64));
        let mut pc = 0usize;
        let mut fuel = self.limits.fuel;

        while pc < code.len() {
            if fuel == 0 {
                return Err(VmError::FuelExhausted);
            }
            fuel -= 1;
            let op_pc = pc;
            let op = code[pc];
            pc += 1;
            match op {
                0x02..=0x08 => push(&mut stack, (op as i32) - 3, self.limits.max_stack)?,
                0x10 => {
                    let b = *code.get(pc).ok_or(VmError::TruncatedInstruction(op_pc))? as i8;
                    pc += 1;
                    push(&mut stack, b as i32, self.limits.max_stack)?;
                }
                0x11 => {
                    let bytes = code
                        .get(pc..pc + 2)
                        .ok_or(VmError::TruncatedInstruction(op_pc))?;
                    pc += 2;
                    push(
                        &mut stack,
                        i16::from_be_bytes([bytes[0], bytes[1]]) as i32,
                        self.limits.max_stack,
                    )?;
                }
                0x15 | 0x36 => {
                    let index = *code.get(pc).ok_or(VmError::TruncatedInstruction(op_pc))? as usize;
                    pc += 1;
                    check_local(index, self.limits.max_locals)?;
                    if op == 0x15 {
                        push(&mut stack, locals[index], self.limits.max_stack)?;
                    } else {
                        locals[index] = pop(&mut stack)?;
                    }
                }
                0x1a..=0x1d => {
                    let index = (op - 0x1a) as usize;
                    check_local(index, self.limits.max_locals)?;
                    push(&mut stack, locals[index], self.limits.max_stack)?;
                }
                0x3b..=0x3e => {
                    let index = (op - 0x3b) as usize;
                    check_local(index, self.limits.max_locals)?;
                    locals[index] = pop(&mut stack)?;
                }
                0x57 => {
                    pop(&mut stack)?;
                }
                0x59 => {
                    let value = *stack.last().ok_or(VmError::StackUnderflow)?;
                    push(&mut stack, value, self.limits.max_stack)?;
                }
                0x60 => binary(&mut stack, self.limits.max_stack, i32::wrapping_add)?,
                0x64 => binary(&mut stack, self.limits.max_stack, i32::wrapping_sub)?,
                0x68 => binary(&mut stack, self.limits.max_stack, i32::wrapping_mul)?,
                0x6c => {
                    let rhs = pop(&mut stack)?;
                    let lhs = pop(&mut stack)?;
                    if rhs == 0 {
                        return Err(VmError::DivisionByZero);
                    }
                    let value = if lhs == i32::MIN && rhs == -1 {
                        i32::MIN
                    } else {
                        lhs / rhs
                    };
                    push(&mut stack, value, self.limits.max_stack)?;
                }
                0x70 => {
                    let rhs = pop(&mut stack)?;
                    let lhs = pop(&mut stack)?;
                    if rhs == 0 {
                        return Err(VmError::DivisionByZero);
                    }
                    push(
                        &mut stack,
                        if lhs == i32::MIN && rhs == -1 {
                            0
                        } else {
                            lhs % rhs
                        },
                        self.limits.max_stack,
                    )?;
                }
                0x74 => {
                    let value = pop(&mut stack)?;
                    push(&mut stack, value.wrapping_neg(), self.limits.max_stack)?;
                }
                0x84 => {
                    let bytes = code
                        .get(pc..pc + 2)
                        .ok_or(VmError::TruncatedInstruction(op_pc))?;
                    pc += 2;
                    let index = bytes[0] as usize;
                    check_local(index, self.limits.max_locals)?;
                    locals[index] = locals[index].wrapping_add(bytes[1] as i8 as i32);
                }
                0x99..=0x9e | 0xa7 => {
                    let bytes = code
                        .get(pc..pc + 2)
                        .ok_or(VmError::TruncatedInstruction(op_pc))?;
                    pc += 2;
                    let offset = i16::from_be_bytes([bytes[0], bytes[1]]) as isize;
                    let take = if op == 0xa7 {
                        true
                    } else {
                        let value = pop(&mut stack)?;
                        match op {
                            0x99 => value == 0,
                            0x9a => value != 0,
                            0x9b => value < 0,
                            0x9c => value >= 0,
                            0x9d => value > 0,
                            0x9e => value <= 0,
                            _ => unreachable!(),
                        }
                    };
                    if take {
                        pc = branch_target(op_pc, offset, code.len())?;
                    }
                }
                0xac => return Ok(ReturnValue::Int(pop(&mut stack)?)),
                0xb1 => {
                    if !stack.is_empty() {
                        return Err(VmError::InvalidReturnState);
                    }
                    return Ok(ReturnValue::Void);
                }
                other => return Err(VmError::UnsupportedOpcode(other, op_pc)),
            }
        }
        Err(VmError::MissingReturn)
    }
}

impl Default for Vm {
    fn default() -> Self {
        Self::new(ExecutionLimits::default())
    }
}

fn push(stack: &mut Vec<i32>, value: i32, limit: usize) -> Result<(), VmError> {
    if stack.len() >= limit {
        return Err(VmError::StackOverflow);
    }
    stack.push(value);
    Ok(())
}
fn pop(stack: &mut Vec<i32>) -> Result<i32, VmError> {
    stack.pop().ok_or(VmError::StackUnderflow)
}
fn check_local(index: usize, limit: usize) -> Result<(), VmError> {
    if index >= limit {
        Err(VmError::InvalidLocal(index))
    } else {
        Ok(())
    }
}
fn binary(stack: &mut Vec<i32>, limit: usize, f: fn(i32, i32) -> i32) -> Result<(), VmError> {
    let rhs = pop(stack)?;
    let lhs = pop(stack)?;
    push(stack, f(lhs, rhs), limit)
}
fn branch_target(origin: usize, offset: isize, code_len: usize) -> Result<usize, VmError> {
    let target = origin as isize + offset;
    if target < 0 || target as usize >= code_len {
        return Err(VmError::InvalidBranchTarget(target));
    }
    Ok(target as usize)
}

/// Result of executing a deliberately small, verified Fabric-JAR subset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModRunReport {
    pub mod_id: String,
    pub initialized_classes: Vec<String>,
    pub instructions_executed: u64,
    /// Messages emitted through the Pomme Java bridge during initialization.
    pub logs: Vec<String>,
}

/// Errors raised while loading or executing the first supported Java-mod tier.
#[derive(Debug, Error)]
pub enum ModExecutionError {
    #[error("invalid mod JAR: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("failed to read mod JAR: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid fabric.mod.json: {0}")]
    Metadata(#[from] serde_json::Error),
    #[error("mod JAR has no fabric.mod.json")]
    MissingFabricMetadata,
    #[error("fabric.mod.json must contain schemaVersion 1 and a non-empty id")]
    InvalidMetadata,
    #[error("fabric.mod.json has no supported client-side entrypoints")]
    NoMainEntrypoints,
    #[error("unsupported Fabric entrypoint value (expected a class-name string)")]
    InvalidEntrypoint,
    #[error("entrypoint class {0} is missing from the JAR")]
    MissingClass(String),
    #[error("referenced class {0} is missing from the mod class path")]
    MissingReferencedClass(String),
    #[error("class-file analysis failed: {0}")]
    ClassFile(#[from] pomme_mod::ClassFileError),
    #[error("class {0} is outside the accepted Java class-file versions (45..=69)")]
    UnsupportedClassVersion(String),
    #[error("class {0} uses field types or class features outside the supported JVM subset")]
    UnsupportedClassShape(String),
    #[error("class {0} must have a supported public no-argument constructor")]
    UnsupportedConstructor(String),
    #[error("class {0} must implement its public Fabric entrypoint method")]
    MissingInitializer(String),
    #[error("class {0} entrypoint contains unsupported bytecode or API calls")]
    UnsupportedInitializer(String),
    #[error("JVM execution failed: {0}")]
    Vm(#[from] VmError),
    #[error("static Java integer helper failed: {0}")]
    IntegerSubset(#[from] pomme_mod::JavaSubsetError),
    #[error("uncaught Java exception {class_name}")]
    JavaThrown { class_name: String, object_id: u32 },
}

#[derive(serde::Deserialize)]
struct FabricModMetadata {
    #[serde(rename = "schemaVersion")]
    schema_version: u32,
    id: String,
    #[serde(default)]
    environment: Option<String>,
    #[serde(default)]
    entrypoints: std::collections::BTreeMap<String, Vec<serde_json::Value>>,
    #[serde(default)]
    jars: Vec<NestedJarMetadata>,
    #[serde(default)]
    provides: Vec<String>,
}

#[derive(serde::Deserialize)]
struct NestedJarMetadata {
    file: String,
}

/// Validate and index a Fabric mod's own classes and declared nested JARs.
/// This is used for library mods that provide classes but no main entrypoint.
pub fn validate_fabric_mod_jar(bytes: &[u8]) -> Result<usize, ModExecutionError> {
    let metadata = read_fabric_metadata(bytes)?;
    let mut classes = BTreeMap::new();
    let mut class_bytes = BTreeMap::new();
    let mut total_class_bytes = 0usize;
    load_mod_classes(
        bytes,
        &metadata.jars,
        &mut classes,
        &mut class_bytes,
        &mut total_class_bytes,
    )?;
    Ok(classes.len())
}

pub fn run_fabric_mod_jar(
    bytes: &[u8],
    fuel_per_entrypoint: u64,
) -> Result<ModRunReport, ModExecutionError> {
    run_fabric_mod_jar_with_classpath(bytes, &[], fuel_per_entrypoint)
}

/// Execute one mod while resolving classes from already-loaded dependency JARs.
/// Dependencies are supplied in loader order; duplicate class definitions are
/// rejected rather than selected nondeterministically.
/// Load and execute the first supported Java Fabric mod from a JAR.
///
/// This milestone intentionally supports the smallest useful Fabric shape:
/// class-file versions 45..=69 are accepted by the parser, but execution is
/// limited to the implemented bytecode subset: a trivial constructor, a public
/// instance onInitialize()V, helper calls, and explicit Pomme/Fabric API
/// bridges. This is not general JVM or Fabric Loader compatibility.
pub fn run_fabric_mod_jar_with_classpath(
    bytes: &[u8],
    dependencies: &[&[u8]],
    fuel_per_entrypoint: u64,
) -> Result<ModRunReport, ModExecutionError> {
    run_fabric_mod_jar_with_classpath_and_version(
        bytes,
        dependencies,
        fuel_per_entrypoint,
        "unknown",
    )
}

/// Execute a mod with the active Minecraft version exposed through the first
/// read-only Minecraft client API bridge methods.
pub fn run_fabric_mod_jar_with_classpath_and_version(
    bytes: &[u8],
    dependencies: &[&[u8]],
    fuel_per_entrypoint: u64,
    minecraft_version: &str,
) -> Result<ModRunReport, ModExecutionError> {
    run_fabric_mod_jar_with_client_context(
        bytes,
        dependencies,
        fuel_per_entrypoint,
        minecraft_version,
        "",
    )
}

/// Execute a mod with read-only Minecraft version and local-user API bridges.
pub fn run_fabric_mod_jar_with_client_context(
    bytes: &[u8],
    dependencies: &[&[u8]],
    fuel_per_entrypoint: u64,
    minecraft_version: &str,
    username: &str,
) -> Result<ModRunReport, ModExecutionError> {
    if dependencies.len() > 512 {
        return Err(ModExecutionError::InvalidMetadata);
    }
    let metadata = read_fabric_metadata(bytes)?;
    let supported_groups = ["preLaunch", "main", "client"];
    let entrypoint_count: usize = supported_groups
        .iter()
        .filter_map(|group| metadata.entrypoints.get(*group))
        .map(Vec::len)
        .sum();
    if entrypoint_count == 0 || entrypoint_count > 256 {
        return Err(ModExecutionError::NoMainEntrypoints);
    }
    if metadata
        .environment
        .as_deref()
        .is_some_and(|environment| !matches!(environment, "*" | "client"))
    {
        return Err(ModExecutionError::NoMainEntrypoints);
    }

    // Build the mod's class path, including nested libraries and classes from
    // already-loaded Fabric dependencies.
    let mut loaded_mod_ids = BTreeSet::new();
    loaded_mod_ids.insert(metadata.id.clone());
    loaded_mod_ids.extend(metadata.provides.iter().cloned());
    let mut classes = BTreeMap::new();
    let mut class_bytes = BTreeMap::new();
    let mut total_class_bytes = 0usize;
    load_mod_classes(
        bytes,
        &metadata.jars,
        &mut classes,
        &mut class_bytes,
        &mut total_class_bytes,
    )?;
    for dependency in dependencies {
        let dependency_metadata = read_fabric_metadata(dependency)?;
        loaded_mod_ids.insert(dependency_metadata.id.clone());
        loaded_mod_ids.extend(dependency_metadata.provides.iter().cloned());
        load_mod_classes(
            dependency,
            &dependency_metadata.jars,
            &mut classes,
            &mut class_bytes,
            &mut total_class_bytes,
        )?;
    }

    let mut initialized_class_names = Vec::with_capacity(entrypoint_count);
    let mut instructions_executed = 0u64;
    let mut logs = Vec::new();
    let mut runtime = FabricRuntimeState::default();
    runtime.loaded_mod_ids = loaded_mod_ids;
    runtime.minecraft_version = minecraft_version.to_owned();
    runtime.minecraft_username = username.to_owned();
    for group in supported_groups {
        let Some(entries) = metadata.entrypoints.get(group) else {
            continue;
        };
        for value in entries {
            let (class_name, entry_environment) = match value {
                serde_json::Value::String(name) => (name.as_str(), None),
                serde_json::Value::Object(object) => {
                    let adapter = object.get("adapter").and_then(serde_json::Value::as_str);
                    if adapter.is_some_and(|adapter| adapter != "default") {
                        return Err(ModExecutionError::InvalidEntrypoint);
                    }
                    (
                        object
                            .get("value")
                            .and_then(serde_json::Value::as_str)
                            .ok_or(ModExecutionError::InvalidEntrypoint)?,
                        object
                            .get("environment")
                            .and_then(serde_json::Value::as_str),
                    )
                }
                _ => return Err(ModExecutionError::InvalidEntrypoint),
            };
            if entry_environment.is_some_and(|environment| !matches!(environment, "*" | "client")) {
                continue;
            }
            let valid_name = !class_name.is_empty()
                && class_name.len() <= 512
                && class_name.split('.').all(|part| {
                    !part.is_empty()
                        && part.bytes().all(|byte| {
                            byte.is_ascii_alphanumeric() || byte == b'_' || byte == 0x24
                        })
                });
            if !valid_name {
                return Err(ModExecutionError::InvalidEntrypoint);
            }
            let internal_name = class_name.replace('.', "/");
            if !classes.contains_key(&internal_name) {
                return Err(ModExecutionError::MissingClass(class_name.to_owned()));
            }
            let (interface_name, method_name) = match group {
                "preLaunch" => (
                    "net/fabricmc/loader/api/entrypoint/PreLaunchEntrypoint",
                    "onPreLaunch",
                ),
                "main" => ("net/fabricmc/api/ModInitializer", "onInitialize"),
                "client" => (
                    "net/fabricmc/api/ClientModInitializer",
                    "onInitializeClient",
                ),
                _ => unreachable!(),
            };
            let (executed, mut class_logs) = execute_minimal_fabric_entrypoint(
                &classes,
                &class_bytes,
                &internal_name,
                class_name,
                interface_name,
                method_name,
                fuel_per_entrypoint,
                &mut runtime,
            )?;
            instructions_executed = instructions_executed.saturating_add(executed);
            logs.append(&mut class_logs);
            initialized_class_names.push(class_name.to_owned());
        }
    }

    Ok(ModRunReport {
        mod_id: metadata.id,
        initialized_classes: initialized_class_names,
        instructions_executed,
        logs,
    })
}

fn read_fabric_metadata(bytes: &[u8]) -> Result<FabricModMetadata, ModExecutionError> {
    use std::io::Read;

    use zip::ZipArchive;

    let mut archive = ZipArchive::new(std::io::Cursor::new(bytes))?;
    if archive.len() > 10_000 {
        return Err(ModExecutionError::InvalidMetadata);
    }
    let file = archive
        .by_name("fabric.mod.json")
        .map_err(|_| ModExecutionError::MissingFabricMetadata)?;
    let mut raw = Vec::new();
    file.take(1024 * 1024 + 1).read_to_end(&mut raw)?;
    if raw.len() > 1024 * 1024 {
        return Err(ModExecutionError::InvalidMetadata);
    }
    let metadata = serde_json::from_slice::<FabricModMetadata>(&raw)?;
    let valid_id = !metadata.id.is_empty()
        && metadata.id.len() <= 64
        && metadata.id.as_bytes()[0].is_ascii_alphabetic()
        && metadata
            .id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-');
    if metadata.schema_version != 1 || !valid_id {
        return Err(ModExecutionError::InvalidMetadata);
    }
    Ok(metadata)
}

fn load_mod_classes(
    bytes: &[u8],
    nested_jars: &[NestedJarMetadata],
    classes: &mut BTreeMap<String, pomme_mod::ClassAnalysis>,
    class_bytes: &mut BTreeMap<String, Vec<u8>>,
    total_class_bytes: &mut usize,
) -> Result<(), ModExecutionError> {
    use std::io::Read;

    use zip::ZipArchive;

    if nested_jars.len() > 256 {
        return Err(ModExecutionError::InvalidMetadata);
    }
    load_jar_classes(bytes, classes, class_bytes, total_class_bytes)?;
    for nested in nested_jars {
        if !safe_nested_jar_path(&nested.file) {
            return Err(ModExecutionError::InvalidMetadata);
        }
        let nested_bytes = {
            let mut outer = ZipArchive::new(std::io::Cursor::new(bytes))?;
            let file = outer
                .by_name(&nested.file)
                .map_err(|_| ModExecutionError::InvalidMetadata)?;
            if file.size() > 64 * 1024 * 1024 {
                return Err(ModExecutionError::InvalidMetadata);
            }
            let mut raw = Vec::new();
            file.take(64 * 1024 * 1024 + 1).read_to_end(&mut raw)?;
            if raw.len() > 64 * 1024 * 1024 {
                return Err(ModExecutionError::InvalidMetadata);
            }
            raw
        };
        load_jar_classes(&nested_bytes, classes, class_bytes, total_class_bytes)?;
    }
    Ok(())
}

fn safe_nested_jar_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

fn load_jar_classes(
    bytes: &[u8],
    classes: &mut BTreeMap<String, pomme_mod::ClassAnalysis>,
    class_bytes: &mut BTreeMap<String, Vec<u8>>,
    total_class_bytes: &mut usize,
) -> Result<(), ModExecutionError> {
    use std::io::Read;

    use zip::ZipArchive;

    let mut archive = ZipArchive::new(std::io::Cursor::new(bytes))?;
    if archive.len() > 10_000 {
        return Err(ModExecutionError::InvalidMetadata);
    }
    for index in 0..archive.len() {
        let file = archive.by_index(index)?;
        let path = file.name()?.into_owned();
        if !path.ends_with(".class")
            || path.starts_with('/')
            || path.contains("../")
            || path.contains('\\')
            || path.starts_with("META-INF/versions/")
        {
            continue;
        }
        let declared = file.size();
        if declared > 32 * 1024 * 1024
            || total_class_bytes.saturating_add(declared as usize) > 64 * 1024 * 1024
        {
            return Err(ModExecutionError::InvalidMetadata);
        }
        let mut raw = Vec::with_capacity(declared as usize);
        file.take(32 * 1024 * 1024 + 1).read_to_end(&mut raw)?;
        if raw.len() > 32 * 1024 * 1024
            || total_class_bytes.saturating_add(raw.len()) > 64 * 1024 * 1024
        {
            return Err(ModExecutionError::InvalidMetadata);
        }
        *total_class_bytes += raw.len();
        if classes.len() >= 4_000 {
            return Err(ModExecutionError::InvalidMetadata);
        }
        let analysis = pomme_mod::analyze_class(&raw)?;
        if !(45..=69).contains(&analysis.major_version) {
            return Err(ModExecutionError::UnsupportedClassVersion(
                analysis.name.clone(),
            ));
        }
        if analysis.name != path.trim_end_matches(".class") {
            return Err(ModExecutionError::InvalidMetadata);
        }
        let internal_name = analysis.name.clone();
        if classes.insert(internal_name.clone(), analysis).is_some() {
            return Err(ModExecutionError::InvalidMetadata);
        }
        class_bytes.insert(internal_name, raw);
    }
    Ok(())
}

fn execute_minimal_fabric_entrypoint(
    classes: &BTreeMap<String, pomme_mod::ClassAnalysis>,
    class_bytes: &BTreeMap<String, Vec<u8>>,
    internal_name: &str,
    display_name: &str,
    interface_name: &str,
    method_name: &str,
    fuel: u64,
    runtime: &mut FabricRuntimeState,
) -> Result<(u64, Vec<String>), ModExecutionError> {
    let class = classes
        .get(internal_name)
        .ok_or_else(|| ModExecutionError::MissingClass(display_name.to_owned()))?;
    if !(45..=69).contains(&class.major_version) {
        return Err(ModExecutionError::UnsupportedClassVersion(
            display_name.to_owned(),
        ));
    }
    if class
        .declared_fields
        .iter()
        .any(|field| field_value_kind(&field.descriptor).is_none())
        || class.access_flags & 0x0001 == 0
        || !class_is_assignable(classes, internal_name, interface_name)
    {
        return Err(ModExecutionError::UnsupportedClassShape(
            display_name.to_owned(),
        ));
    }

    let constructor = class
        .methods
        .iter()
        .find(|method| method.name == "<init>" && method.descriptor == "()V")
        .ok_or_else(|| ModExecutionError::UnsupportedConstructor(display_name.to_owned()))?;
    let valid_constructor = constructor.access_flags & 0x0001 != 0
        && constructor.access_flags & 0x0008 == 0
        && constructor.descriptor == "()V"
        && constructor.invoked_methods.iter().all(|call| {
            call.owner == "java/lang/Object" && call.name == "<init>" && call.descriptor == "()V"
        });
    if !valid_constructor {
        return Err(ModExecutionError::UnsupportedConstructor(
            display_name.to_owned(),
        ));
    }

    let initializer = class
        .methods
        .iter()
        .find(|method| method.name == method_name && method.descriptor == "()V")
        .ok_or_else(|| ModExecutionError::MissingInitializer(display_name.to_owned()))?;
    if initializer.access_flags & 0x0001 == 0 || initializer.access_flags & 0x0008 != 0 {
        return Err(ModExecutionError::MissingInitializer(
            display_name.to_owned(),
        ));
    }

    if runtime.object_classes.len() >= 65_536 {
        return Err(ModExecutionError::UnsupportedInitializer(
            display_name.to_owned(),
        ));
    }
    let receiver_id = runtime.next_object_id;
    runtime.next_object_id = runtime
        .next_object_id
        .checked_add(1)
        .ok_or_else(|| ModExecutionError::UnsupportedInitializer(display_name.to_owned()))?;
    runtime
        .object_classes
        .insert(receiver_id, internal_name.to_owned());
    let receiver = FabricStackValue::Object(receiver_id);
    let mut fuel_left = fuel;
    let mut instructions = 0u64;
    let mut logs = Vec::new();
    initialize_fabric_class(
        classes,
        class_bytes,
        internal_name,
        display_name,
        &mut fuel_left,
        &mut instructions,
        &mut logs,
        runtime,
        0,
    )?;
    execute_fabric_method(
        classes,
        class_bytes,
        class,
        constructor,
        std::slice::from_ref(&receiver),
        display_name,
        &mut fuel_left,
        &mut instructions,
        &mut logs,
        runtime,
        0,
    )?;
    execute_fabric_method(
        classes,
        class_bytes,
        class,
        initializer,
        std::slice::from_ref(&receiver),
        display_name,
        &mut fuel_left,
        &mut instructions,
        &mut logs,
        runtime,
        0,
    )?;
    Ok((instructions, logs))
}

#[derive(Clone)]
enum FabricStackValue {
    Object(u32),
    String(String),
    Null,
    Int(i32),
}

#[derive(Default)]
struct FabricRuntimeState {
    static_fields: BTreeMap<(String, String), FabricStackValue>,
    instance_fields: BTreeMap<(u32, String, String), FabricStackValue>,
    initialized_classes: BTreeSet<String>,
    object_classes: BTreeMap<u32, String>,
    minecraft_client_id: Option<u32>,
    minecraft_user_id: Option<u32>,
    minecraft_version: String,
    minecraft_username: String,
    arrays: BTreeMap<u32, Vec<FabricStackValue>>,
    array_elements: usize,
    logger_objects: BTreeMap<u32, String>,
    fabric_loader_objects: BTreeSet<u32>,
    fabric_loader_id: Option<u32>,
    client_env_id: Option<u32>,
    client_env_objects: BTreeSet<u32>,
    loaded_mod_ids: BTreeSet<String>,
    next_object_id: u32,
}

fn builtin_java_superclass(name: &str) -> Option<&'static str> {
    match name {
        "java/lang/Throwable" => Some("java/lang/Object"),
        "java/lang/Exception" | "java/lang/Error" => Some("java/lang/Throwable"),
        "java/lang/RuntimeException" => Some("java/lang/Exception"),
        "java/lang/IllegalArgumentException"
        | "java/lang/IllegalStateException"
        | "java/lang/NullPointerException"
        | "java/lang/ArithmeticException"
        | "java/lang/IndexOutOfBoundsException"
        | "java/lang/ClassCastException"
        | "java/lang/NegativeArraySizeException"
        | "java/lang/UnsupportedOperationException" => Some("java/lang/RuntimeException"),
        "java/lang/ArrayIndexOutOfBoundsException" => Some("java/lang/IndexOutOfBoundsException"),
        "java/io/IOException" => Some("java/lang/Exception"),
        _ => None,
    }
}

fn is_builtin_throwable_class(name: &str) -> bool {
    name == "java/lang/Throwable" || builtin_java_superclass(name).is_some()
}

fn class_is_assignable(
    classes: &BTreeMap<String, pomme_mod::ClassAnalysis>,
    actual: &str,
    expected: &str,
) -> bool {
    fn visit(
        classes: &BTreeMap<String, pomme_mod::ClassAnalysis>,
        actual: &str,
        expected: &str,
        seen: &mut BTreeSet<String>,
        depth: usize,
    ) -> bool {
        if actual == expected {
            return true;
        }
        if depth >= 64 || !seen.insert(actual.to_owned()) {
            return false;
        }
        let Some(class) = classes.get(actual) else {
            return builtin_java_superclass(actual)
                .is_some_and(|super_name| visit(classes, super_name, expected, seen, depth + 1));
        };
        if class.interfaces.iter().any(|interface| {
            interface == expected || visit(classes, interface, expected, seen, depth + 1)
        }) {
            return true;
        }
        class
            .super_name
            .as_deref()
            .is_some_and(|super_name| visit(classes, super_name, expected, seen, depth + 1))
    }
    visit(classes, actual, expected, &mut BTreeSet::new(), 0)
}

fn fabric_value_is_instance_of(
    classes: &BTreeMap<String, pomme_mod::ClassAnalysis>,
    runtime: &FabricRuntimeState,
    value: &FabricStackValue,
    target: &str,
) -> bool {
    match value {
        FabricStackValue::Null => false,
        FabricStackValue::String(_) => target == "java/lang/String" || target == "java/lang/Object",
        FabricStackValue::Object(id) => {
            let Some(actual) = runtime.object_classes.get(id) else {
                return false;
            };
            if actual.starts_with('[') {
                target == "java/lang/Object"
                    || target == "java/lang/Cloneable"
                    || target == "java/io/Serializable"
                    || actual == target
            } else {
                class_is_assignable(classes, actual, target)
            }
        }
        FabricStackValue::Int(_) => false,
    }
}

fn resolve_field_declaration<'a>(
    classes: &'a BTreeMap<String, pomme_mod::ClassAnalysis>,
    start: &str,
    name: &str,
    descriptor: &str,
    is_static: bool,
) -> Option<(String, &'a pomme_mod::FieldSummary)> {
    let mut current = Some(start);
    for _ in 0..64 {
        let class_name = current?;
        let class = classes.get(class_name)?;
        if let Some(field) = class.declared_fields.iter().find(|field| {
            field.name == name
                && field.descriptor == descriptor
                && (field.access_flags & 0x0008 != 0) == is_static
        }) {
            return Some((class_name.to_owned(), field));
        }
        current = class.super_name.as_deref();
    }
    None
}

fn resolve_instance_method<'a>(
    classes: &'a BTreeMap<String, pomme_mod::ClassAnalysis>,
    start: &str,
    name: &str,
    descriptor: &str,
) -> Option<(&'a pomme_mod::ClassAnalysis, &'a pomme_mod::MethodSummary)> {
    let mut current = Some(start);
    for _ in 0..64 {
        let class_name = current?;
        let class = classes.get(class_name)?;
        if let Some(method) = class.methods.iter().find(|method| {
            method.name == name
                && method.descriptor == descriptor
                && method.access_flags & 0x0008 == 0
        }) {
            return Some((class, method));
        }
        current = class.super_name.as_deref();
    }
    None
}

fn parse_method_descriptor(descriptor: &str) -> Option<(Vec<char>, char)> {
    let mut rest = descriptor.strip_prefix('(')?;
    let mut parameters = Vec::new();
    while !rest.starts_with(')') {
        let first = rest.chars().next()?;
        match first {
            'B' | 'C' | 'I' | 'S' | 'Z' => {
                parameters.push('I');
                rest = &rest[1..];
            }
            'L' => {
                let end = rest.find(';')?;
                let name = &rest[1..end];
                parameters.push(if name == "java/lang/String" { 'S' } else { 'R' });
                rest = &rest[end + 1..];
            }
            '[' => {
                while rest.starts_with('[') {
                    rest = &rest[1..];
                }
                if rest.starts_with('L') {
                    let end = rest.find(';')?;
                    rest = &rest[end + 1..];
                } else if matches!(
                    rest.chars().next()?,
                    'B' | 'C' | 'I' | 'S' | 'Z' | 'F' | 'J' | 'D'
                ) {
                    rest = &rest[1..];
                } else {
                    return None;
                }
                parameters.push('R');
            }
            _ => return None,
        }
    }
    rest = rest.strip_prefix(')')?;
    let result = if rest == "V" {
        'V'
    } else if matches!(rest, "B" | "C" | "I" | "S" | "Z") {
        'I'
    } else if rest == "Ljava/lang/String;" {
        'S'
    } else if rest.starts_with('L') || rest.starts_with('[') {
        'R'
    } else {
        return None;
    };
    Some((parameters, result))
}

fn pop_typed_arguments(
    stack: &mut Vec<FabricStackValue>,
    kinds: &[char],
    display_name: &str,
) -> Result<Vec<FabricStackValue>, ModExecutionError> {
    let mut arguments = Vec::with_capacity(kinds.len());
    for kind in kinds.iter().rev() {
        let value = stack
            .pop()
            .ok_or_else(|| ModExecutionError::UnsupportedInitializer(display_name.to_owned()))?;
        let valid = match kind {
            'I' => matches!(value, FabricStackValue::Int(_)),
            'S' => matches!(value, FabricStackValue::String(_) | FabricStackValue::Null),
            'R' => matches!(
                value,
                FabricStackValue::Object(_) | FabricStackValue::String(_) | FabricStackValue::Null
            ),
            _ => false,
        };
        if !valid {
            return Err(ModExecutionError::UnsupportedInitializer(
                display_name.to_owned(),
            ));
        }
        arguments.push(value);
    }
    arguments.reverse();
    Ok(arguments)
}

fn field_value_kind(descriptor: &str) -> Option<char> {
    if matches!(descriptor, "B" | "C" | "I" | "S" | "Z") {
        Some('I')
    } else if descriptor == "Ljava/lang/String;" {
        Some('S')
    } else if descriptor.starts_with('L') || descriptor.starts_with('[') {
        Some('R')
    } else {
        None
    }
}

fn default_field_value(kind: char) -> FabricStackValue {
    if kind == 'I' {
        FabricStackValue::Int(0)
    } else {
        FabricStackValue::Null
    }
}

fn stack_value_matches_kind(value: &FabricStackValue, kind: char) -> bool {
    match kind {
        'I' => matches!(value, FabricStackValue::Int(_)),
        'S' => matches!(value, FabricStackValue::String(_) | FabricStackValue::Null),
        'R' => matches!(
            value,
            FabricStackValue::Object(_) | FabricStackValue::String(_) | FabricStackValue::Null
        ),
        _ => false,
    }
}

fn validate_return_value(
    result: Option<FabricStackValue>,
    kind: char,
    display_name: &str,
) -> Result<Option<FabricStackValue>, ModExecutionError> {
    let valid = match &result {
        None => kind == 'V',
        Some(value) => stack_value_matches_kind(value, kind),
    };
    if valid {
        Ok(result)
    } else {
        Err(ModExecutionError::UnsupportedInitializer(
            display_name.to_owned(),
        ))
    }
}

fn concat_argument_kinds(descriptor: &str) -> Option<Vec<char>> {
    let mut rest = descriptor.strip_prefix('(')?;
    let mut kinds = Vec::new();
    while !rest.starts_with(')') {
        let kind = rest.chars().next()?;
        match kind {
            'B' | 'C' | 'I' | 'S' | 'Z' => {
                kinds.push(kind);
                rest = &rest[1..];
            }
            'L' => {
                let end = rest.find(';')?;
                let object_type = &rest[..=end];
                if object_type != "Ljava/lang/String;" {
                    return None;
                }
                kinds.push('L');
                rest = &rest[end + 1..];
            }
            _ => return None,
        }
    }
    let result = rest.strip_prefix(')')?;
    result.starts_with("Ljava/lang/String;").then_some(kinds)
}

fn bootstrap_constant_string(class: &pomme_mod::ClassAnalysis, index: u16) -> Option<String> {
    class
        .string_constants
        .get(&index)
        .cloned()
        .or_else(|| class.integer_constants.get(&index).map(ToString::to_string))
}

/// Run a class initializer once, immediately before the first active use.
fn initialize_fabric_class(
    classes: &BTreeMap<String, pomme_mod::ClassAnalysis>,
    class_bytes: &BTreeMap<String, Vec<u8>>,
    internal_name: &str,
    display_name: &str,
    fuel_left: &mut u64,
    instructions: &mut u64,
    logs: &mut Vec<String>,
    runtime: &mut FabricRuntimeState,
    depth: usize,
) -> Result<(), ModExecutionError> {
    if depth > 64 {
        return Err(ModExecutionError::UnsupportedInitializer(
            display_name.to_owned(),
        ));
    }
    if !runtime.initialized_classes.insert(internal_name.to_owned()) {
        return Ok(());
    }
    let class = classes
        .get(internal_name)
        .ok_or_else(|| ModExecutionError::MissingReferencedClass(internal_name.to_owned()))?;
    if let Some(super_name) = class.super_name.as_deref()
        && super_name != "java/lang/Object"
        && classes.contains_key(super_name)
    {
        initialize_fabric_class(
            classes,
            class_bytes,
            super_name,
            display_name,
            fuel_left,
            instructions,
            logs,
            runtime,
            depth + 1,
        )?;
    }
    for field in &class.declared_fields {
        if field.access_flags & 0x0008 != 0
            && field.descriptor == "I"
            && let Some(value) = field.constant_value
        {
            runtime.static_fields.insert(
                (internal_name.to_owned(), field.name.clone()),
                FabricStackValue::Int(value),
            );
        }
    }
    if let Some(initializer) = class
        .methods
        .iter()
        .find(|method| method.name == "<clinit>" && method.descriptor == "()V")
    {
        execute_fabric_method(
            classes,
            class_bytes,
            class,
            initializer,
            &[],
            display_name,
            fuel_left,
            instructions,
            logs,
            runtime,
            depth + 1,
        )?;
    }
    Ok(())
}

fn fabric_branch_target_wide(
    instruction_pc: usize,
    offset: i32,
    code_len: usize,
    display_name: &str,
) -> Result<usize, ModExecutionError> {
    let target = instruction_pc as i64 + i64::from(offset);
    if target < 0 || target >= code_len as i64 {
        return Err(ModExecutionError::UnsupportedInitializer(
            display_name.to_owned(),
        ));
    }
    Ok(target as usize)
}

fn fabric_branch_target(
    instruction_pc: usize,
    offset: i16,
    code_len: usize,
    display_name: &str,
) -> Result<usize, ModExecutionError> {
    let target = instruction_pc as i64 + i64::from(offset);
    if target < 0 || target >= code_len as i64 {
        return Err(ModExecutionError::UnsupportedInitializer(
            display_name.to_owned(),
        ));
    }
    Ok(target as usize)
}

fn handle_fabric_exception(
    classes: &BTreeMap<String, pomme_mod::ClassAnalysis>,
    method: &pomme_mod::MethodSummary,
    stack: &mut Vec<FabricStackValue>,
    pc: &mut usize,
    instruction_pc: usize,
    exception_class: &str,
    object_id: u32,
    _display_name: &str,
) -> Result<(), ModExecutionError> {
    let handler = method.exception_handlers.iter().find(|handler| {
        usize::from(handler.start_pc) <= instruction_pc
            && instruction_pc < usize::from(handler.end_pc)
            && handler
                .catch_type
                .as_deref()
                .is_none_or(|catch_type| class_is_assignable(classes, exception_class, catch_type))
    });
    if let Some(handler) = handler {
        stack.clear();
        stack.push(FabricStackValue::Object(object_id));
        *pc = usize::from(handler.handler_pc);
        Ok(())
    } else {
        Err(ModExecutionError::JavaThrown {
            class_name: exception_class.to_owned(),
            object_id,
        })
    }
}

fn allocate_builtin_exception(
    runtime: &mut FabricRuntimeState,
    exception_class: &str,
    display_name: &str,
) -> Result<u32, ModExecutionError> {
    if !is_builtin_throwable_class(exception_class) || runtime.object_classes.len() >= 65_536 {
        return Err(ModExecutionError::UnsupportedInitializer(
            display_name.to_owned(),
        ));
    }
    let object_id = runtime.next_object_id;
    runtime.next_object_id = runtime
        .next_object_id
        .checked_add(1)
        .ok_or_else(|| ModExecutionError::UnsupportedInitializer(display_name.to_owned()))?;
    runtime
        .object_classes
        .insert(object_id, exception_class.to_owned());
    Ok(object_id)
}

/// Execute a small, verified method subset. Calls resolve against the mod and
/// dependency class path; Java library calls remain restricted to explicit
/// Pomme bridge methods.
fn execute_fabric_method(
    classes: &BTreeMap<String, pomme_mod::ClassAnalysis>,
    class_bytes: &BTreeMap<String, Vec<u8>>,
    class: &pomme_mod::ClassAnalysis,
    method: &pomme_mod::MethodSummary,
    initial_locals: &[FabricStackValue],
    display_name: &str,
    fuel_left: &mut u64,
    instructions: &mut u64,
    logs: &mut Vec<String>,
    runtime: &mut FabricRuntimeState,
    depth: usize,
) -> Result<Option<FabricStackValue>, ModExecutionError> {
    if depth > 64 {
        return Err(ModExecutionError::UnsupportedInitializer(
            display_name.to_owned(),
        ));
    }

    let code = &method.code;
    let mut pc = 0usize;
    if method.max_stack > 1024 || method.max_locals > 256 {
        return Err(ModExecutionError::UnsupportedInitializer(
            display_name.to_owned(),
        ));
    }
    if initial_locals.len() > usize::from(method.max_locals) {
        return Err(ModExecutionError::UnsupportedInitializer(
            display_name.to_owned(),
        ));
    }
    let mut locals = vec![None::<FabricStackValue>; usize::from(method.max_locals)];
    for (slot, value) in initial_locals.iter().cloned().enumerate() {
        locals[slot] = Some(value);
    }
    let mut stack: Vec<FabricStackValue> = Vec::with_capacity(usize::from(method.max_stack));

    loop {
        if *fuel_left == 0 {
            return Err(ModExecutionError::Vm(VmError::FuelExhausted));
        }
        *fuel_left -= 1;
        *instructions = instructions.saturating_add(1);

        let instruction_pc = pc;
        let opcode = *code
            .get(pc)
            .ok_or_else(|| ModExecutionError::UnsupportedInitializer(display_name.to_owned()))?;
        pc += 1;
        match opcode {
            0x00 => {}
            0x01 => {
                if stack.len() >= usize::from(method.max_stack) {
                    return Err(ModExecutionError::UnsupportedInitializer(
                        display_name.to_owned(),
                    ));
                }
                stack.push(FabricStackValue::Null);
            }
            0x02..=0x08 => {
                if stack.len() >= usize::from(method.max_stack) {
                    return Err(ModExecutionError::UnsupportedInitializer(
                        display_name.to_owned(),
                    ));
                }
                stack.push(FabricStackValue::Int(i32::from(opcode) - 3));
            }
            0x10 => {
                let value = *code.get(pc).ok_or_else(|| {
                    ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                })? as i8 as i32;
                pc += 1;
                if stack.len() >= usize::from(method.max_stack) {
                    return Err(ModExecutionError::UnsupportedInitializer(
                        display_name.to_owned(),
                    ));
                }
                stack.push(FabricStackValue::Int(value));
            }
            0x11 => {
                let bytes = code.get(pc..pc + 2).ok_or_else(|| {
                    ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                })?;
                pc += 2;
                let value = i16::from_be_bytes([bytes[0], bytes[1]]) as i32;
                if stack.len() >= usize::from(method.max_stack) {
                    return Err(ModExecutionError::UnsupportedInitializer(
                        display_name.to_owned(),
                    ));
                }
                stack.push(FabricStackValue::Int(value));
            }
            0x15 | 0x1a..=0x1d => {
                let index = if opcode == 0x15 {
                    let index = *code.get(pc).ok_or_else(|| {
                        ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                    })? as usize;
                    pc += 1;
                    index
                } else {
                    usize::from(opcode - 0x1a)
                };
                let Some(FabricStackValue::Int(value)) = locals.get(index).and_then(Option::as_ref)
                else {
                    return Err(ModExecutionError::UnsupportedInitializer(
                        display_name.to_owned(),
                    ));
                };
                if stack.len() >= usize::from(method.max_stack) {
                    return Err(ModExecutionError::UnsupportedInitializer(
                        display_name.to_owned(),
                    ));
                }
                stack.push(FabricStackValue::Int(*value));
            }
            0x36 | 0x3b..=0x3e => {
                let index = if opcode == 0x36 {
                    let index = *code.get(pc).ok_or_else(|| {
                        ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                    })? as usize;
                    pc += 1;
                    index
                } else {
                    usize::from(opcode - 0x3b)
                };
                let Some(FabricStackValue::Int(value)) = stack.pop() else {
                    return Err(ModExecutionError::UnsupportedInitializer(
                        display_name.to_owned(),
                    ));
                };
                let slot = locals.get_mut(index).ok_or_else(|| {
                    ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                })?;
                *slot = Some(FabricStackValue::Int(value));
            }
            0x84 => {
                let operands = code.get(pc..pc + 2).ok_or_else(|| {
                    ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                })?;
                pc += 2;
                let index = usize::from(operands[0]);
                let increment = i32::from(operands[1] as i8);
                let Some(Some(FabricStackValue::Int(value))) = locals.get_mut(index) else {
                    return Err(ModExecutionError::UnsupportedInitializer(
                        display_name.to_owned(),
                    ));
                };
                *value = value.wrapping_add(increment);
            }
            0x99..=0x9e => {
                let bytes = code.get(pc..pc + 2).ok_or_else(|| {
                    ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                })?;
                pc += 2;
                let Some(FabricStackValue::Int(value)) = stack.pop() else {
                    return Err(ModExecutionError::UnsupportedInitializer(
                        display_name.to_owned(),
                    ));
                };
                let take = match opcode {
                    0x99 => value == 0,
                    0x9a => value != 0,
                    0x9b => value < 0,
                    0x9c => value >= 0,
                    0x9d => value > 0,
                    _ => value <= 0,
                };
                if take {
                    pc = fabric_branch_target(
                        instruction_pc,
                        i16::from_be_bytes([bytes[0], bytes[1]]),
                        code.len(),
                        display_name,
                    )?;
                }
            }
            0x9f..=0xa4 => {
                let bytes = code.get(pc..pc + 2).ok_or_else(|| {
                    ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                })?;
                pc += 2;
                let Some(FabricStackValue::Int(rhs)) = stack.pop() else {
                    return Err(ModExecutionError::UnsupportedInitializer(
                        display_name.to_owned(),
                    ));
                };
                let Some(FabricStackValue::Int(lhs)) = stack.pop() else {
                    return Err(ModExecutionError::UnsupportedInitializer(
                        display_name.to_owned(),
                    ));
                };
                let take = match opcode {
                    0x9f => lhs == rhs,
                    0xa0 => lhs != rhs,
                    0xa1 => lhs < rhs,
                    0xa2 => lhs >= rhs,
                    0xa3 => lhs > rhs,
                    _ => lhs <= rhs,
                };
                if take {
                    pc = fabric_branch_target(
                        instruction_pc,
                        i16::from_be_bytes([bytes[0], bytes[1]]),
                        code.len(),
                        display_name,
                    )?;
                }
            }
            0xa5 | 0xa6 => {
                let bytes = code.get(pc..pc + 2).ok_or_else(|| {
                    ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                })?;
                pc += 2;
                let rhs = stack.pop().ok_or_else(|| {
                    ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                })?;
                let lhs = stack.pop().ok_or_else(|| {
                    ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                })?;
                let equal = match (&lhs, &rhs) {
                    (FabricStackValue::Object(a), FabricStackValue::Object(b)) => a == b,
                    (FabricStackValue::Null, FabricStackValue::Null) => true,
                    (FabricStackValue::String(a), FabricStackValue::String(b)) => a == b,
                    _ => false,
                };
                if equal == (opcode == 0xa5) {
                    pc = fabric_branch_target(
                        instruction_pc,
                        i16::from_be_bytes([bytes[0], bytes[1]]),
                        code.len(),
                        display_name,
                    )?;
                }
            }
            0xa7 => {
                let bytes = code.get(pc..pc + 2).ok_or_else(|| {
                    ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                })?;
                pc = fabric_branch_target(
                    instruction_pc,
                    i16::from_be_bytes([bytes[0], bytes[1]]),
                    code.len(),
                    display_name,
                )?;
            }
            0xc6 | 0xc7 => {
                let bytes = code.get(pc..pc + 2).ok_or_else(|| {
                    ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                })?;
                pc += 2;
                let value = stack.pop().ok_or_else(|| {
                    ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                })?;
                let is_null = matches!(value, FabricStackValue::Null);
                if is_null == (opcode == 0xc6) {
                    pc = fabric_branch_target(
                        instruction_pc,
                        i16::from_be_bytes([bytes[0], bytes[1]]),
                        code.len(),
                        display_name,
                    )?;
                }
            }
            0xbf => {
                let Some(FabricStackValue::Object(object_id)) = stack.pop() else {
                    return Err(ModExecutionError::UnsupportedInitializer(
                        display_name.to_owned(),
                    ));
                };
                let exception_class =
                    runtime
                        .object_classes
                        .get(&object_id)
                        .cloned()
                        .ok_or_else(|| {
                            ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                        })?;
                if !class_is_assignable(classes, &exception_class, "java/lang/Throwable") {
                    return Err(ModExecutionError::UnsupportedInitializer(
                        display_name.to_owned(),
                    ));
                }
                handle_fabric_exception(
                    classes,
                    method,
                    &mut stack,
                    &mut pc,
                    instruction_pc,
                    &exception_class,
                    object_id,
                    display_name,
                )?;
            }
            0xc0 | 0xc1 => {
                let bytes = code.get(pc..pc + 2).ok_or_else(|| {
                    ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                })?;
                pc += 2;
                let index = u16::from_be_bytes([bytes[0], bytes[1]]);
                let target = class.class_constants.get(&index).ok_or_else(|| {
                    ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                })?;
                let value = stack.pop().ok_or_else(|| {
                    ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                })?;
                let matches = fabric_value_is_instance_of(classes, runtime, &value, target);
                if opcode == 0xc0 {
                    if !matches && !matches!(value, FabricStackValue::Null) {
                        return Err(ModExecutionError::UnsupportedInitializer(
                            display_name.to_owned(),
                        ));
                    }
                    stack.push(value);
                } else {
                    stack.push(FabricStackValue::Int(i32::from(matches)));
                }
            }
            0xaa => {
                while pc % 4 != 0 {
                    pc += 1;
                }
                let header = code.get(pc..pc + 12).ok_or_else(|| {
                    ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                })?;
                let default = i32::from_be_bytes(header[0..4].try_into().unwrap());
                let low = i32::from_be_bytes(header[4..8].try_into().unwrap());
                let high = i32::from_be_bytes(header[8..12].try_into().unwrap());
                if high < low || i64::from(high) - i64::from(low) > 65_535 {
                    return Err(ModExecutionError::UnsupportedInitializer(
                        display_name.to_owned(),
                    ));
                }
                pc += 12;
                let count = (i64::from(high) - i64::from(low) + 1) as usize;
                let table_bytes = count.checked_mul(4).ok_or_else(|| {
                    ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                })?;
                let table = code.get(pc..pc + table_bytes).ok_or_else(|| {
                    ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                })?;
                let Some(FabricStackValue::Int(key)) = stack.pop() else {
                    return Err(ModExecutionError::UnsupportedInitializer(
                        display_name.to_owned(),
                    ));
                };
                let offset = if key >= low && key <= high {
                    let slot = (i64::from(key) - i64::from(low)) as usize * 4;
                    i32::from_be_bytes(table[slot..slot + 4].try_into().unwrap())
                } else {
                    default
                };
                pc = fabric_branch_target_wide(instruction_pc, offset, code.len(), display_name)?;
            }
            0xab => {
                while pc % 4 != 0 {
                    pc += 1;
                }
                let header = code.get(pc..pc + 8).ok_or_else(|| {
                    ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                })?;
                let default = i32::from_be_bytes(header[0..4].try_into().unwrap());
                let pairs = i32::from_be_bytes(header[4..8].try_into().unwrap());
                if !(0..=65_535).contains(&pairs) {
                    return Err(ModExecutionError::UnsupportedInitializer(
                        display_name.to_owned(),
                    ));
                }
                pc += 8;
                let table_bytes = (pairs as usize).checked_mul(8).ok_or_else(|| {
                    ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                })?;
                let table = code.get(pc..pc + table_bytes).ok_or_else(|| {
                    ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                })?;
                let Some(FabricStackValue::Int(key)) = stack.pop() else {
                    return Err(ModExecutionError::UnsupportedInitializer(
                        display_name.to_owned(),
                    ));
                };
                let mut offset = default;
                let mut previous = None;
                for pair in table.chunks_exact(8) {
                    let match_key = i32::from_be_bytes(pair[0..4].try_into().unwrap());
                    if previous.is_some_and(|value| match_key <= value) {
                        return Err(ModExecutionError::UnsupportedInitializer(
                            display_name.to_owned(),
                        ));
                    }
                    previous = Some(match_key);
                    if match_key == key {
                        offset = i32::from_be_bytes(pair[4..8].try_into().unwrap());
                        break;
                    }
                }
                pc = fabric_branch_target_wide(instruction_pc, offset, code.len(), display_name)?;
            }
            0x57 => {
                stack.pop().ok_or_else(|| {
                    ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                })?;
            }
            0x59 => {
                let value = stack.last().cloned().ok_or_else(|| {
                    ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                })?;
                if stack.len() >= usize::from(method.max_stack) {
                    return Err(ModExecutionError::UnsupportedInitializer(
                        display_name.to_owned(),
                    ));
                }
                stack.push(value);
            }
            0x2e | 0x32..=0x35 => {
                let Some(FabricStackValue::Int(index)) = stack.pop() else {
                    return Err(ModExecutionError::UnsupportedInitializer(
                        display_name.to_owned(),
                    ));
                };
                let Some(FabricStackValue::Object(array_id)) = stack.pop() else {
                    return Err(ModExecutionError::UnsupportedInitializer(
                        display_name.to_owned(),
                    ));
                };
                let array_type = runtime.object_classes.get(&array_id).ok_or_else(|| {
                    ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                })?;
                let value = runtime
                    .arrays
                    .get(&array_id)
                    .and_then(|array| {
                        usize::try_from(index)
                            .ok()
                            .and_then(|index| array.get(index))
                    })
                    .cloned()
                    .ok_or_else(|| {
                        ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                    })?;
                let value = match (opcode, array_type.as_str(), value) {
                    (0x2e, "[I", FabricStackValue::Int(value)) => FabricStackValue::Int(value),
                    (0x32, ty, value)
                        if (ty.starts_with("[L") || ty.starts_with("[["))
                            && matches!(
                                value,
                                FabricStackValue::Object(_)
                                    | FabricStackValue::String(_)
                                    | FabricStackValue::Null
                            ) =>
                    {
                        value
                    }
                    (0x33, "[B", FabricStackValue::Int(value)) => {
                        FabricStackValue::Int(i32::from(value as i8))
                    }
                    (0x33, "[Z", FabricStackValue::Int(value)) => {
                        FabricStackValue::Int(i32::from(value != 0))
                    }
                    (0x34, "[C", FabricStackValue::Int(value)) => {
                        FabricStackValue::Int(i32::from(value as u16))
                    }
                    (0x35, "[S", FabricStackValue::Int(value)) => {
                        FabricStackValue::Int(i32::from(value as i16))
                    }
                    _ => {
                        return Err(ModExecutionError::UnsupportedInitializer(
                            display_name.to_owned(),
                        ));
                    }
                };
                stack.push(value);
            }
            0x4f | 0x53 | 0x54 | 0x55 | 0x56 => {
                let value = stack.pop().ok_or_else(|| {
                    ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                })?;
                let Some(FabricStackValue::Int(index)) = stack.pop() else {
                    return Err(ModExecutionError::UnsupportedInitializer(
                        display_name.to_owned(),
                    ));
                };
                let Some(FabricStackValue::Object(array_id)) = stack.pop() else {
                    return Err(ModExecutionError::UnsupportedInitializer(
                        display_name.to_owned(),
                    ));
                };
                let array_type =
                    runtime
                        .object_classes
                        .get(&array_id)
                        .cloned()
                        .ok_or_else(|| {
                            ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                        })?;
                let value = match (opcode, array_type.as_str(), value) {
                    (0x4f, "[I", FabricStackValue::Int(value)) => FabricStackValue::Int(value),
                    (0x53, ty, value)
                        if (ty.starts_with("[L") || ty.starts_with("[["))
                            && matches!(
                                value,
                                FabricStackValue::Object(_)
                                    | FabricStackValue::String(_)
                                    | FabricStackValue::Null
                            ) =>
                    {
                        value
                    }
                    (0x54, "[B", FabricStackValue::Int(value)) => {
                        FabricStackValue::Int(i32::from(value as i8))
                    }
                    (0x54, "[Z", FabricStackValue::Int(value)) => {
                        FabricStackValue::Int(i32::from(value != 0))
                    }
                    (0x55, "[C", FabricStackValue::Int(value)) => {
                        FabricStackValue::Int(i32::from(value as u16))
                    }
                    (0x56, "[S", FabricStackValue::Int(value)) => {
                        FabricStackValue::Int(i32::from(value as i16))
                    }
                    _ => {
                        return Err(ModExecutionError::UnsupportedInitializer(
                            display_name.to_owned(),
                        ));
                    }
                };
                let slot = runtime
                    .arrays
                    .get_mut(&array_id)
                    .and_then(|array| {
                        usize::try_from(index)
                            .ok()
                            .and_then(|index| array.get_mut(index))
                    })
                    .ok_or_else(|| {
                        ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                    })?;
                *slot = value;
            }
            0xbe => {
                let Some(FabricStackValue::Object(array_id)) = stack.pop() else {
                    return Err(ModExecutionError::UnsupportedInitializer(
                        display_name.to_owned(),
                    ));
                };
                let length = runtime
                    .arrays
                    .get(&array_id)
                    .and_then(|array| i32::try_from(array.len()).ok())
                    .ok_or_else(|| {
                        ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                    })?;
                stack.push(FabricStackValue::Int(length));
            }
            0xbc | 0xbd => {
                let array_type = if opcode == 0xbc {
                    let atype = *code.get(pc).ok_or_else(|| {
                        ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                    })?;
                    pc += 1;
                    match atype {
                        4 => "[Z".to_owned(),
                        5 => "[C".to_owned(),
                        8 => "[B".to_owned(),
                        9 => "[S".to_owned(),
                        10 => "[I".to_owned(),
                        _ => {
                            return Err(ModExecutionError::UnsupportedInitializer(
                                display_name.to_owned(),
                            ));
                        }
                    }
                } else {
                    let bytes = code.get(pc..pc + 2).ok_or_else(|| {
                        ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                    })?;
                    pc += 2;
                    let index = u16::from_be_bytes([bytes[0], bytes[1]]);
                    let component = class.class_constants.get(&index).ok_or_else(|| {
                        ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                    })?;
                    format!("[L{component};")
                };
                let Some(FabricStackValue::Int(length)) = stack.pop() else {
                    return Err(ModExecutionError::UnsupportedInitializer(
                        display_name.to_owned(),
                    ));
                };
                if length < 0
                    || length > 1_000_000
                    || runtime.object_classes.len() >= 65_536
                    || runtime.array_elements.saturating_add(length as usize) > 1_000_000
                {
                    return Err(ModExecutionError::UnsupportedInitializer(
                        display_name.to_owned(),
                    ));
                }
                let array_id = runtime.next_object_id;
                runtime.next_object_id =
                    runtime.next_object_id.checked_add(1).ok_or_else(|| {
                        ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                    })?;
                let initial = if array_type.starts_with("[L") || array_type.starts_with("[[") {
                    FabricStackValue::Null
                } else {
                    FabricStackValue::Int(0)
                };
                runtime.array_elements += length as usize;
                runtime
                    .arrays
                    .insert(array_id, vec![initial; length as usize]);
                runtime.object_classes.insert(array_id, array_type);
                stack.push(FabricStackValue::Object(array_id));
            }
            0x91..=0x93 => {
                let Some(FabricStackValue::Int(value)) = stack.pop() else {
                    return Err(ModExecutionError::UnsupportedInitializer(
                        display_name.to_owned(),
                    ));
                };
                let converted = match opcode {
                    0x91 => i32::from(value as i8),
                    0x92 => i32::from(value as u16),
                    _ => i32::from(value as i16),
                };
                stack.push(FabricStackValue::Int(converted));
            }
            0x74 => {
                let Some(FabricStackValue::Int(value)) = stack.pop() else {
                    return Err(ModExecutionError::UnsupportedInitializer(
                        display_name.to_owned(),
                    ));
                };
                stack.push(FabricStackValue::Int(value.wrapping_neg()));
            }
            0x6c | 0x70 | 0x7e | 0x80 | 0x82 | 0x78 | 0x7a | 0x7c => {
                let Some(FabricStackValue::Int(rhs)) = stack.pop() else {
                    return Err(ModExecutionError::UnsupportedInitializer(
                        display_name.to_owned(),
                    ));
                };
                let Some(FabricStackValue::Int(lhs)) = stack.pop() else {
                    return Err(ModExecutionError::UnsupportedInitializer(
                        display_name.to_owned(),
                    ));
                };
                if matches!(opcode, 0x6c | 0x70) && rhs == 0 {
                    let object_id = allocate_builtin_exception(
                        runtime,
                        "java/lang/ArithmeticException",
                        display_name,
                    )?;
                    handle_fabric_exception(
                        classes,
                        method,
                        &mut stack,
                        &mut pc,
                        instruction_pc,
                        "java/lang/ArithmeticException",
                        object_id,
                        display_name,
                    )?;
                } else {
                    let value = match opcode {
                        0x6c => lhs.wrapping_div(rhs),
                        0x70 => lhs.wrapping_rem(rhs),
                        0x7e => lhs & rhs,
                        0x80 => lhs | rhs,
                        0x82 => lhs ^ rhs,
                        0x78 => lhs.wrapping_shl((rhs as u32) & 31),
                        0x7a => lhs.wrapping_shr((rhs as u32) & 31),
                        _ => ((lhs as u32) >> ((rhs as u32) & 31)) as i32,
                    };
                    stack.push(FabricStackValue::Int(value));
                }
            }
            0x60 | 0x64 | 0x68 => {
                let Some(FabricStackValue::Int(rhs)) = stack.pop() else {
                    return Err(ModExecutionError::UnsupportedInitializer(
                        display_name.to_owned(),
                    ));
                };
                let Some(FabricStackValue::Int(lhs)) = stack.pop() else {
                    return Err(ModExecutionError::UnsupportedInitializer(
                        display_name.to_owned(),
                    ));
                };
                let value = match opcode {
                    0x60 => lhs.wrapping_add(rhs),
                    0x64 => lhs.wrapping_sub(rhs),
                    _ => lhs.wrapping_mul(rhs),
                };
                stack.push(FabricStackValue::Int(value));
            }
            0x2a..=0x2d => {
                let index = usize::from(opcode - 0x2a);
                let value = locals
                    .get(index)
                    .and_then(Option::as_ref)
                    .cloned()
                    .ok_or_else(|| {
                        ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                    })?;
                if stack.len() >= usize::from(method.max_stack) {
                    return Err(ModExecutionError::UnsupportedInitializer(
                        display_name.to_owned(),
                    ));
                }
                stack.push(value);
            }
            0x19 => {
                let index = *code.get(pc).ok_or_else(|| {
                    ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                })? as usize;
                pc += 1;
                let value = locals
                    .get(index)
                    .and_then(Option::as_ref)
                    .cloned()
                    .ok_or_else(|| {
                        ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                    })?;
                if stack.len() >= usize::from(method.max_stack) {
                    return Err(ModExecutionError::UnsupportedInitializer(
                        display_name.to_owned(),
                    ));
                }
                stack.push(value);
            }
            0x4b..=0x4e => {
                let index = usize::from(opcode - 0x4b);
                let value = stack.pop().ok_or_else(|| {
                    ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                })?;
                let slot = locals.get_mut(index).ok_or_else(|| {
                    ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                })?;
                *slot = Some(value);
            }
            0x3a => {
                let index = *code.get(pc).ok_or_else(|| {
                    ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                })? as usize;
                pc += 1;
                let value = stack.pop().ok_or_else(|| {
                    ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                })?;
                let slot = locals.get_mut(index).ok_or_else(|| {
                    ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                })?;
                *slot = Some(value);
            }
            0x12 | 0x13 => {
                let index = if opcode == 0x12 {
                    let index = *code.get(pc).ok_or_else(|| {
                        ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                    })? as u16;
                    pc += 1;
                    index
                } else {
                    let bytes = code.get(pc..pc + 2).ok_or_else(|| {
                        ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                    })?;
                    pc += 2;
                    u16::from_be_bytes([bytes[0], bytes[1]])
                };
                let value = if let Some(value) = class.string_constants.get(&index) {
                    value.clone()
                } else if let Some(class_name) = class.class_constants.get(&index) {
                    class_name.clone()
                } else {
                    return Err(ModExecutionError::UnsupportedInitializer(
                        display_name.to_owned(),
                    ));
                };
                if stack.len() >= usize::from(method.max_stack) {
                    return Err(ModExecutionError::UnsupportedInitializer(
                        display_name.to_owned(),
                    ));
                }
                stack.push(FabricStackValue::String(value));
            }
            0xb2 | 0xb3 => {
                let bytes = code.get(pc..pc + 2).ok_or_else(|| {
                    ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                })?;
                pc += 2;
                let index = u16::from_be_bytes([bytes[0], bytes[1]]);
                let reference = class.field_reference_constants.get(&index).ok_or_else(|| {
                    ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                })?;
                let kind = field_value_kind(&reference.descriptor).ok_or_else(|| {
                    ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                })?;
                let (declaring_owner, _) = resolve_field_declaration(
                    classes,
                    &reference.owner,
                    &reference.name,
                    &reference.descriptor,
                    true,
                )
                .ok_or_else(|| {
                    ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                })?;
                initialize_fabric_class(
                    classes,
                    class_bytes,
                    &reference.owner,
                    display_name,
                    fuel_left,
                    instructions,
                    logs,
                    runtime,
                    depth + 1,
                )?;
                let key = (declaring_owner.to_owned(), reference.name.clone());
                if opcode == 0xb2 {
                    let value = runtime
                        .static_fields
                        .get(&key)
                        .cloned()
                        .unwrap_or_else(|| default_field_value(kind));
                    if !stack_value_matches_kind(&value, kind)
                        || stack.len() >= usize::from(method.max_stack)
                    {
                        return Err(ModExecutionError::UnsupportedInitializer(
                            display_name.to_owned(),
                        ));
                    }
                    stack.push(value);
                } else {
                    let value = stack.pop().ok_or_else(|| {
                        ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                    })?;
                    if !stack_value_matches_kind(&value, kind) {
                        return Err(ModExecutionError::UnsupportedInitializer(
                            display_name.to_owned(),
                        ));
                    }
                    runtime.static_fields.insert(key, value);
                }
            }
            0xb4 | 0xb5 => {
                let bytes = code.get(pc..pc + 2).ok_or_else(|| {
                    ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                })?;
                pc += 2;
                let index = u16::from_be_bytes([bytes[0], bytes[1]]);
                let reference = class.field_reference_constants.get(&index).ok_or_else(|| {
                    ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                })?;
                let kind = field_value_kind(&reference.descriptor).ok_or_else(|| {
                    ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                })?;
                let (declaring_owner, _) = resolve_field_declaration(
                    classes,
                    &reference.owner,
                    &reference.name,
                    &reference.descriptor,
                    false,
                )
                .ok_or_else(|| {
                    ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                })?;
                if opcode == 0xb4 {
                    let Some(FabricStackValue::Object(object_id)) = stack.pop() else {
                        return Err(ModExecutionError::UnsupportedInitializer(
                            display_name.to_owned(),
                        ));
                    };
                    let actual_class = runtime.object_classes.get(&object_id).ok_or_else(|| {
                        ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                    })?;
                    if !class_is_assignable(classes, actual_class, &reference.owner) {
                        return Err(ModExecutionError::UnsupportedInitializer(
                            display_name.to_owned(),
                        ));
                    }
                    let key = (
                        object_id,
                        declaring_owner.to_owned(),
                        reference.name.clone(),
                    );
                    let value = runtime
                        .instance_fields
                        .get(&key)
                        .cloned()
                        .unwrap_or_else(|| default_field_value(kind));
                    if !stack_value_matches_kind(&value, kind)
                        || stack.len() >= usize::from(method.max_stack)
                    {
                        return Err(ModExecutionError::UnsupportedInitializer(
                            display_name.to_owned(),
                        ));
                    }
                    stack.push(value);
                } else {
                    let value = stack.pop().ok_or_else(|| {
                        ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                    })?;
                    if !stack_value_matches_kind(&value, kind) {
                        return Err(ModExecutionError::UnsupportedInitializer(
                            display_name.to_owned(),
                        ));
                    }
                    let Some(FabricStackValue::Object(object_id)) = stack.pop() else {
                        return Err(ModExecutionError::UnsupportedInitializer(
                            display_name.to_owned(),
                        ));
                    };
                    let actual_class = runtime.object_classes.get(&object_id).ok_or_else(|| {
                        ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                    })?;
                    if !class_is_assignable(classes, actual_class, &reference.owner) {
                        return Err(ModExecutionError::UnsupportedInitializer(
                            display_name.to_owned(),
                        ));
                    }
                    let key = (
                        object_id,
                        declaring_owner.to_owned(),
                        reference.name.clone(),
                    );
                    runtime.instance_fields.insert(key, value);
                }
            }
            0xba => {
                let bytes = code.get(pc..pc + 4).ok_or_else(|| {
                    ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                })?;
                pc += 4;
                if bytes[2] != 0 || bytes[3] != 0 {
                    return Err(ModExecutionError::UnsupportedInitializer(
                        display_name.to_owned(),
                    ));
                }
                let index = u16::from_be_bytes([bytes[0], bytes[1]]);
                let call_site = class.invokedynamic_references.get(&index).ok_or_else(|| {
                    ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                })?;
                let bootstrap = class
                    .bootstrap_method_details
                    .get(call_site.bootstrap_index)
                    .ok_or_else(|| {
                        ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                    })?;
                if bootstrap.target.owner != "java/lang/invoke/StringConcatFactory"
                    || !matches!(
                        bootstrap.target.name.as_str(),
                        "makeConcat" | "makeConcatWithConstants"
                    )
                {
                    return Err(ModExecutionError::UnsupportedInitializer(
                        display_name.to_owned(),
                    ));
                }
                let kinds = concat_argument_kinds(&call_site.descriptor).ok_or_else(|| {
                    ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                })?;
                let mut arguments = Vec::with_capacity(kinds.len());
                for kind in kinds.iter().rev() {
                    let value = match kind {
                        'B' | 'C' | 'I' | 'S' | 'Z' => match stack.pop() {
                            Some(FabricStackValue::Int(value)) => value.to_string(),
                            _ => {
                                return Err(ModExecutionError::UnsupportedInitializer(
                                    display_name.to_owned(),
                                ));
                            }
                        },
                        'L' => match stack.pop() {
                            Some(FabricStackValue::String(value)) => value,
                            _ => {
                                return Err(ModExecutionError::UnsupportedInitializer(
                                    display_name.to_owned(),
                                ));
                            }
                        },
                        _ => {
                            return Err(ModExecutionError::UnsupportedInitializer(
                                display_name.to_owned(),
                            ));
                        }
                    };
                    arguments.push(value);
                }
                arguments.reverse();
                let mut output = String::new();
                if bootstrap.target.name == "makeConcat" {
                    output.push_str(&arguments.concat());
                } else {
                    let recipe_index = *bootstrap.arguments.first().ok_or_else(|| {
                        ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                    })?;
                    let recipe = class.string_constants.get(&recipe_index).ok_or_else(|| {
                        ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                    })?;
                    let mut dynamic_index = 0usize;
                    let mut constant_index = 1usize;
                    for character in recipe.chars() {
                        match character {
                            '' => {
                                output.push_str(arguments.get(dynamic_index).ok_or_else(|| {
                                    ModExecutionError::UnsupportedInitializer(
                                        display_name.to_owned(),
                                    )
                                })?);
                                dynamic_index += 1;
                            }
                            '' => {
                                let constant =
                                    *bootstrap.arguments.get(constant_index).ok_or_else(|| {
                                        ModExecutionError::UnsupportedInitializer(
                                            display_name.to_owned(),
                                        )
                                    })?;
                                output.push_str(
                                    &bootstrap_constant_string(class, constant).ok_or_else(
                                        || {
                                            ModExecutionError::UnsupportedInitializer(
                                                display_name.to_owned(),
                                            )
                                        },
                                    )?,
                                );
                                constant_index += 1;
                            }
                            value => output.push(value),
                        }
                    }
                    if dynamic_index != arguments.len()
                        || constant_index != bootstrap.arguments.len()
                    {
                        return Err(ModExecutionError::UnsupportedInitializer(
                            display_name.to_owned(),
                        ));
                    }
                }
                if stack.len() >= usize::from(method.max_stack) {
                    return Err(ModExecutionError::UnsupportedInitializer(
                        display_name.to_owned(),
                    ));
                }
                stack.push(FabricStackValue::String(output));
            }
            0xbb => {
                let bytes = code.get(pc..pc + 2).ok_or_else(|| {
                    ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                })?;
                pc += 2;
                let index = u16::from_be_bytes([bytes[0], bytes[1]]);
                let target_name = class.class_constants.get(&index).ok_or_else(|| {
                    ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                })?;
                if !classes.contains_key(target_name) || runtime.object_classes.len() >= 65_536 {
                    return Err(ModExecutionError::MissingReferencedClass(
                        target_name.clone(),
                    ));
                }
                initialize_fabric_class(
                    classes,
                    class_bytes,
                    target_name,
                    display_name,
                    fuel_left,
                    instructions,
                    logs,
                    runtime,
                    depth + 1,
                )?;
                let object_id = runtime.next_object_id;
                runtime.next_object_id =
                    runtime.next_object_id.checked_add(1).ok_or_else(|| {
                        ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                    })?;
                runtime
                    .object_classes
                    .insert(object_id, target_name.clone());
                stack.push(FabricStackValue::Object(object_id));
            }
            0xb8 => {
                let bytes = code.get(pc..pc + 2).ok_or_else(|| {
                    ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                })?;
                pc += 2;
                let index = u16::from_be_bytes([bytes[0], bytes[1]]);
                let reference = class.method_references.get(&index).ok_or_else(|| {
                    ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                })?;
                if reference.owner == "net/minecraft/client/Minecraft"
                    && reference.name == "getInstance"
                    && reference.descriptor == "()Lnet/minecraft/client/Minecraft;"
                {
                    let id = if let Some(id) = runtime.minecraft_client_id {
                        id
                    } else {
                        if runtime.object_classes.len() >= 65_536 {
                            return Err(ModExecutionError::UnsupportedInitializer(
                                display_name.to_owned(),
                            ));
                        }
                        let id = runtime.next_object_id;
                        runtime.next_object_id =
                            runtime.next_object_id.checked_add(1).ok_or_else(|| {
                                ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                            })?;
                        runtime
                            .object_classes
                            .insert(id, "net/minecraft/client/Minecraft".to_owned());
                        runtime.minecraft_client_id = Some(id);
                        id
                    };
                    stack.push(FabricStackValue::Object(id));
                    continue;
                }
                if reference.owner == "pomme/api/Pomme"
                    && reference.name == "log"
                    && reference.descriptor == "(Ljava/lang/String;)V"
                {
                    let Some(FabricStackValue::String(message)) = stack.pop() else {
                        return Err(ModExecutionError::UnsupportedInitializer(
                            display_name.to_owned(),
                        ));
                    };
                    logs.push(message);
                    continue;
                }
                if reference.owner == "pomme/api/Pomme"
                    && reference.name == "logInt"
                    && reference.descriptor == "(I)V"
                {
                    let Some(FabricStackValue::Int(value)) = stack.pop() else {
                        return Err(ModExecutionError::UnsupportedInitializer(
                            display_name.to_owned(),
                        ));
                    };
                    logs.push(value.to_string());
                    continue;
                }
                if reference.owner == "net/fabricmc/loader/api/FabricLoader"
                    && reference.name == "getInstance"
                    && reference.descriptor == "()Lnet/fabricmc/loader/api/FabricLoader;"
                {
                    let loader_id = if let Some(id) = runtime.fabric_loader_id {
                        id
                    } else {
                        if runtime.object_classes.len() >= 65_536 {
                            return Err(ModExecutionError::UnsupportedInitializer(
                                display_name.to_owned(),
                            ));
                        }
                        let id = runtime.next_object_id;
                        runtime.next_object_id =
                            runtime.next_object_id.checked_add(1).ok_or_else(|| {
                                ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                            })?;
                        runtime
                            .object_classes
                            .insert(id, "net/fabricmc/loader/api/FabricLoader".to_owned());
                        runtime.fabric_loader_objects.insert(id);
                        runtime.fabric_loader_id = Some(id);
                        id
                    };
                    stack.push(FabricStackValue::Object(loader_id));
                    continue;
                }
                if reference.owner == "org/slf4j/LoggerFactory"
                    && reference.name == "getLogger"
                    && matches!(
                        reference.descriptor.as_str(),
                        "(Ljava/lang/String;)Lorg/slf4j/Logger;"
                            | "(Ljava/lang/Class;)Lorg/slf4j/Logger;"
                    )
                {
                    let Some(FabricStackValue::String(logger_name)) = stack.pop() else {
                        return Err(ModExecutionError::UnsupportedInitializer(
                            display_name.to_owned(),
                        ));
                    };
                    if runtime.object_classes.len() >= 65_536 {
                        return Err(ModExecutionError::UnsupportedInitializer(
                            display_name.to_owned(),
                        ));
                    }
                    let logger_id = runtime.next_object_id;
                    runtime.next_object_id =
                        runtime.next_object_id.checked_add(1).ok_or_else(|| {
                            ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                        })?;
                    runtime
                        .object_classes
                        .insert(logger_id, "org/slf4j/Logger".to_owned());
                    runtime.logger_objects.insert(logger_id, logger_name);
                    stack.push(FabricStackValue::Object(logger_id));
                    continue;
                }
                let (parameter_kinds, return_kind) = parse_method_descriptor(&reference.descriptor)
                    .ok_or_else(|| {
                        ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                    })?;
                let arguments = pop_typed_arguments(&mut stack, &parameter_kinds, display_name)?;
                let target_class = classes.get(&reference.owner).ok_or_else(|| {
                    ModExecutionError::MissingReferencedClass(reference.owner.clone())
                })?;
                let target = target_class
                    .methods
                    .iter()
                    .find(|candidate| {
                        candidate.name == reference.name
                            && candidate.descriptor == reference.descriptor
                            && candidate.access_flags & 0x0008 != 0
                    })
                    .ok_or_else(|| {
                        ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                    })?;
                initialize_fabric_class(
                    classes,
                    class_bytes,
                    &reference.owner,
                    display_name,
                    fuel_left,
                    instructions,
                    logs,
                    runtime,
                    depth + 1,
                )?;
                let result = execute_fabric_method(
                    classes,
                    class_bytes,
                    target_class,
                    target,
                    &arguments,
                    display_name,
                    fuel_left,
                    instructions,
                    logs,
                    runtime,
                    depth + 1,
                )?;
                let result = validate_return_value(result, return_kind, display_name)?;
                if let Some(value) = result {
                    if stack.len() >= usize::from(method.max_stack) {
                        return Err(ModExecutionError::UnsupportedInitializer(
                            display_name.to_owned(),
                        ));
                    }
                    stack.push(value);
                }
            }
            0xb6 | 0xb7 | 0xb9 => {
                let operand_len = if opcode == 0xb9 { 4 } else { 2 };
                let bytes = code.get(pc..pc + operand_len).ok_or_else(|| {
                    ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                })?;
                pc += operand_len;
                if opcode == 0xb9 && (bytes[3] != 0 || bytes[2] == 0) {
                    return Err(ModExecutionError::UnsupportedInitializer(
                        display_name.to_owned(),
                    ));
                }
                let index = u16::from_be_bytes([bytes[0], bytes[1]]);
                let reference = class.method_references.get(&index).ok_or_else(|| {
                    ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                })?;
                if (opcode == 0xb9) != reference.interface_call {
                    return Err(ModExecutionError::UnsupportedInitializer(
                        display_name.to_owned(),
                    ));
                }
                let (parameter_kinds, return_kind) = parse_method_descriptor(&reference.descriptor)
                    .ok_or_else(|| {
                        ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                    })?;
                if opcode == 0xb9 && usize::from(bytes[2]) != parameter_kinds.len() + 1 {
                    return Err(ModExecutionError::UnsupportedInitializer(
                        display_name.to_owned(),
                    ));
                }
                let mut arguments =
                    pop_typed_arguments(&mut stack, &parameter_kinds, display_name)?;
                let receiver = stack.pop().ok_or_else(|| {
                    ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                })?;
                let object_id = match receiver {
                    FabricStackValue::Object(id) => id,
                    _ => {
                        return Err(ModExecutionError::UnsupportedInitializer(
                            display_name.to_owned(),
                        ));
                    }
                };
                if reference.name == "<init>"
                    && (reference.owner == "java/lang/Object"
                        || is_builtin_throwable_class(&reference.owner))
                {
                    continue;
                }
                if reference.name == "<init>" {
                    if opcode != 0xb7 || reference.interface_call {
                        return Err(ModExecutionError::UnsupportedInitializer(
                            display_name.to_owned(),
                        ));
                    }
                    let actual_class = runtime.object_classes.get(&object_id).ok_or_else(|| {
                        ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                    })?;
                    if !class_is_assignable(classes, actual_class, &reference.owner) {
                        return Err(ModExecutionError::UnsupportedInitializer(
                            display_name.to_owned(),
                        ));
                    }
                    let target_class = classes.get(&reference.owner).ok_or_else(|| {
                        ModExecutionError::MissingReferencedClass(reference.owner.clone())
                    })?;
                    let constructor = target_class
                        .methods
                        .iter()
                        .find(|candidate| {
                            candidate.name == "<init>"
                                && candidate.descriptor == reference.descriptor
                        })
                        .ok_or_else(|| {
                            ModExecutionError::UnsupportedConstructor(reference.owner.clone())
                        })?;
                    let mut locals = vec![receiver];
                    locals.append(&mut arguments);
                    execute_fabric_method(
                        classes,
                        class_bytes,
                        target_class,
                        constructor,
                        &locals,
                        display_name,
                        fuel_left,
                        instructions,
                        logs,
                        runtime,
                        depth + 1,
                    )?;
                    continue;
                }
                if runtime.client_env_objects.contains(&object_id)
                    && reference.owner == "net/fabricmc/api/EnvType"
                    && reference.name == "isClient"
                    && reference.descriptor == "()Z"
                {
                    stack.push(FabricStackValue::Int(1));
                    continue;
                }
                if runtime.client_env_objects.contains(&object_id)
                    && reference.owner == "net/fabricmc/api/EnvType"
                    && reference.name == "isServer"
                    && reference.descriptor == "()Z"
                {
                    stack.push(FabricStackValue::Int(0));
                    continue;
                }
                if runtime.minecraft_client_id == Some(object_id)
                    && reference.owner == "net/minecraft/client/Minecraft"
                    && reference.name == "getLaunchedVersion"
                    && reference.descriptor == "()Ljava/lang/String;"
                {
                    stack.push(FabricStackValue::String(runtime.minecraft_version.clone()));
                    continue;
                }
                if runtime.minecraft_client_id == Some(object_id)
                    && reference.owner == "net/minecraft/client/Minecraft"
                    && reference.name == "getUser"
                    && reference.descriptor == "()Lnet/minecraft/client/User;"
                {
                    let id = if let Some(id) = runtime.minecraft_user_id {
                        id
                    } else {
                        if runtime.object_classes.len() >= 65_536 {
                            return Err(ModExecutionError::UnsupportedInitializer(
                                display_name.to_owned(),
                            ));
                        }
                        let id = runtime.next_object_id;
                        runtime.next_object_id =
                            runtime.next_object_id.checked_add(1).ok_or_else(|| {
                                ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                            })?;
                        runtime
                            .object_classes
                            .insert(id, "net/minecraft/client/User".to_owned());
                        runtime.minecraft_user_id = Some(id);
                        id
                    };
                    stack.push(FabricStackValue::Object(id));
                    continue;
                }
                if runtime.minecraft_user_id == Some(object_id)
                    && reference.owner == "net/minecraft/client/User"
                    && reference.name == "getName"
                    && reference.descriptor == "()Ljava/lang/String;"
                {
                    stack.push(FabricStackValue::String(runtime.minecraft_username.clone()));
                    continue;
                }
                if runtime.fabric_loader_objects.contains(&object_id)
                    && reference.owner == "net/fabricmc/loader/api/FabricLoader"
                    && reference.name == "getEnvironmentType"
                    && reference.descriptor == "()Lnet/fabricmc/api/EnvType;"
                {
                    let env_id = if let Some(id) = runtime.client_env_id {
                        id
                    } else {
                        if runtime.object_classes.len() >= 65_536 {
                            return Err(ModExecutionError::UnsupportedInitializer(
                                display_name.to_owned(),
                            ));
                        }
                        let id = runtime.next_object_id;
                        runtime.next_object_id =
                            runtime.next_object_id.checked_add(1).ok_or_else(|| {
                                ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                            })?;
                        runtime
                            .object_classes
                            .insert(id, "net/fabricmc/api/EnvType".to_owned());
                        runtime.client_env_objects.insert(id);
                        runtime.client_env_id = Some(id);
                        id
                    };
                    stack.push(FabricStackValue::Object(env_id));
                    continue;
                }
                if runtime.fabric_loader_objects.contains(&object_id)
                    && reference.owner == "net/fabricmc/loader/api/FabricLoader"
                    && reference.name == "isModLoaded"
                    && reference.descriptor == "(Ljava/lang/String;)Z"
                {
                    let Some(FabricStackValue::String(mod_id)) = arguments.first() else {
                        return Err(ModExecutionError::UnsupportedInitializer(
                            display_name.to_owned(),
                        ));
                    };
                    stack.push(FabricStackValue::Int(i32::from(
                        runtime.loaded_mod_ids.contains(mod_id),
                    )));
                    continue;
                }
                if runtime.fabric_loader_objects.contains(&object_id)
                    && reference.owner == "net/fabricmc/loader/api/FabricLoader"
                    && reference.name == "isDevelopmentEnvironment"
                    && reference.descriptor == "()Z"
                {
                    stack.push(FabricStackValue::Int(0));
                    continue;
                }
                if let Some(logger_name) = runtime.logger_objects.get(&object_id)
                    && reference.owner == "org/slf4j/Logger"
                    && matches!(
                        reference.name.as_str(),
                        "info" | "warn" | "error" | "debug" | "trace"
                    )
                    && reference.descriptor == "(Ljava/lang/String;)V"
                {
                    let Some(FabricStackValue::String(message)) = arguments.first() else {
                        return Err(ModExecutionError::UnsupportedInitializer(
                            display_name.to_owned(),
                        ));
                    };
                    logs.push(format!("{logger_name}: {message}"));
                    continue;
                }
                let dynamic_name = runtime.object_classes.get(&object_id).ok_or_else(|| {
                    ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                })?;
                if !class_is_assignable(classes, dynamic_name, &reference.owner) {
                    return Err(ModExecutionError::UnsupportedInitializer(
                        display_name.to_owned(),
                    ));
                }
                let lookup_start = if opcode == 0xb7 {
                    &reference.owner
                } else {
                    dynamic_name
                };
                let (target_class, target) = resolve_instance_method(
                    classes,
                    lookup_start,
                    &reference.name,
                    &reference.descriptor,
                )
                .ok_or_else(|| {
                    ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                })?;
                let mut locals = vec![receiver];
                locals.append(&mut arguments);
                let result = execute_fabric_method(
                    classes,
                    class_bytes,
                    target_class,
                    target,
                    &locals,
                    display_name,
                    fuel_left,
                    instructions,
                    logs,
                    runtime,
                    depth + 1,
                )?;
                let result = validate_return_value(result, return_kind, display_name)?;
                if let Some(value) = result {
                    if stack.len() >= usize::from(method.max_stack) {
                        return Err(ModExecutionError::UnsupportedInitializer(
                            display_name.to_owned(),
                        ));
                    }
                    stack.push(value);
                }
            }
            0xac => {
                if !method.descriptor.ends_with(")I") {
                    return Err(ModExecutionError::UnsupportedInitializer(
                        display_name.to_owned(),
                    ));
                }
                let Some(FabricStackValue::Int(value)) = stack.pop() else {
                    return Err(ModExecutionError::UnsupportedInitializer(
                        display_name.to_owned(),
                    ));
                };
                if !stack.is_empty() {
                    return Err(ModExecutionError::UnsupportedInitializer(
                        display_name.to_owned(),
                    ));
                }
                return Ok(Some(FabricStackValue::Int(value)));
            }
            0xb0 => {
                let value = stack.pop().ok_or_else(|| {
                    ModExecutionError::UnsupportedInitializer(display_name.to_owned())
                })?;
                if !method.descriptor.ends_with(")Ljava/lang/String;")
                    || !matches!(value, FabricStackValue::String(_))
                    || !stack.is_empty()
                {
                    return Err(ModExecutionError::UnsupportedInitializer(
                        display_name.to_owned(),
                    ));
                }
                return Ok(Some(value));
            }
            0xb1 => {
                if !method.descriptor.ends_with(")V") || !stack.is_empty() {
                    return Err(ModExecutionError::UnsupportedInitializer(
                        display_name.to_owned(),
                    ));
                }
                return Ok(None);
            }
            _ => {
                return Err(ModExecutionError::UnsupportedInitializer(
                    display_name.to_owned(),
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiny_fabric_mod_jar() -> Vec<u8> {
        use std::io::Write;

        use zip::write::SimpleFileOptions;

        fn u2(out: &mut Vec<u8>, value: u16) {
            out.extend(value.to_be_bytes());
        }
        fn u4(out: &mut Vec<u8>, value: u32) {
            out.extend(value.to_be_bytes());
        }
        fn utf8(out: &mut Vec<u8>, value: &str) {
            out.push(1);
            u2(out, value.len() as u16);
            out.extend(value.as_bytes());
        }

        // Java 21 class ExampleMod: public ExampleMod() { super(); }
        // public void onInitialize() { }.
        let mut class = Vec::new();
        u4(&mut class, 0xcafebabe);
        u2(&mut class, 0);
        u2(&mut class, 65);
        u2(&mut class, 24);
        utf8(&mut class, "example/ExampleMod"); // #1
        class.push(7);
        u2(&mut class, 1); // #2 Class
        utf8(&mut class, "java/lang/Object"); // #3
        class.push(7);
        u2(&mut class, 3); // #4 Class
        utf8(&mut class, "<init>"); // #5
        utf8(&mut class, "()V"); // #6
        class.push(12);
        u2(&mut class, 5);
        u2(&mut class, 6); // #7 NameAndType
        class.push(10);
        u2(&mut class, 4);
        u2(&mut class, 7); // #8 Methodref
        utf8(&mut class, "Code"); // #9
        utf8(&mut class, "onInitialize"); // #10
        utf8(&mut class, "pomme/api/Pomme"); // #11
        class.push(7);
        u2(&mut class, 11); // #12 Class
        utf8(&mut class, "log"); // #13
        utf8(&mut class, "(Ljava/lang/String;)V"); // #14
        class.push(12);
        u2(&mut class, 13);
        u2(&mut class, 14); // #15 NameAndType
        class.push(10);
        u2(&mut class, 12);
        u2(&mut class, 15); // #16 Methodref
        utf8(&mut class, "Hello from Java"); // #17
        class.push(8);
        u2(&mut class, 17); // #18 String
        utf8(&mut class, "net/fabricmc/api/ModInitializer"); // #19
        class.push(7);
        u2(&mut class, 19); // #20 Class
        utf8(&mut class, "logFromHelper"); // #21
        class.push(12);
        u2(&mut class, 21);
        u2(&mut class, 6); // #22 NameAndType
        class.push(10);
        u2(&mut class, 2);
        u2(&mut class, 22); // #23 Methodref

        u2(&mut class, 0x0021); // public super
        u2(&mut class, 2); // this
        u2(&mut class, 4); // super
        u2(&mut class, 1); // interfaces
        u2(&mut class, 20); // ModInitializer
        u2(&mut class, 0); // fields
        u2(&mut class, 3); // methods

        // Public no-arg constructor: aload_0; invokespecial Object.<init>; return.
        u2(&mut class, 0x0001);
        u2(&mut class, 5);
        u2(&mut class, 6);
        u2(&mut class, 1);
        u2(&mut class, 9);
        u4(&mut class, 17);
        u2(&mut class, 1);
        u2(&mut class, 1);
        u4(&mut class, 5);
        class.extend([0x2a, 0xb7, 0x00, 0x08, 0xb1]);
        u2(&mut class, 0);
        u2(&mut class, 0);

        // Public instance initializer delegates to a private instance method.
        u2(&mut class, 0x0001);
        u2(&mut class, 10);
        u2(&mut class, 6);
        u2(&mut class, 1);
        u2(&mut class, 9);
        u4(&mut class, 17);
        u2(&mut class, 1);
        u2(&mut class, 1);
        u4(&mut class, 5);
        class.extend([0x2a, 0xb7, 0x00, 0x17, 0xb1]);
        u2(&mut class, 0);
        u2(&mut class, 0);

        // Private helper: ldc "Hello from Java"; invokestatic Pomme.log; return.
        u2(&mut class, 0x0002);
        u2(&mut class, 21);
        u2(&mut class, 6);
        u2(&mut class, 1);
        u2(&mut class, 9);
        u4(&mut class, 18);
        u2(&mut class, 1);
        u2(&mut class, 1);
        u4(&mut class, 6);
        class.extend([0x12, 0x12, 0xb8, 0x00, 0x10, 0xb1]);
        u2(&mut class, 0);
        u2(&mut class, 0);
        u2(&mut class, 0); // class attributes

        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        writer
            .start_file("fabric.mod.json", SimpleFileOptions::default())
            .unwrap();
        writer.write_all(br#"{"schemaVersion":1,"id":"tiny_example","version":"1.0.0","entrypoints":{"main":["example.ExampleMod"]}}"#).unwrap();
        writer
            .start_file("example/ExampleMod.class", SimpleFileOptions::default())
            .unwrap();
        writer.write_all(&class).unwrap();
        writer.finish().unwrap().into_inner()
    }

    #[test]
    fn nested_jar_paths_reject_traversal_and_absolute_paths() {
        for path in [
            "../outside.jar",
            "META-INF/../../outside.jar",
            "/tmp/evil.jar",
            "C:\\evil.jar",
            "META-INF\\evil.jar",
        ] {
            assert!(!safe_nested_jar_path(path), "unsafe path accepted: {path}");
        }
        assert!(safe_nested_jar_path("META-INF/jars/library.jar"));
    }

    #[test]
    fn loads_a_fabric_jar_and_executes_a_java_on_initialize_method() {
        let jar = tiny_fabric_mod_jar();
        let report = run_fabric_mod_jar(&jar, 100).unwrap();
        assert_eq!(report.mod_id, "tiny_example");
        assert_eq!(report.initialized_classes, ["example.ExampleMod"]);
        assert_eq!(report.instructions_executed, 9);
        assert_eq!(report.logs, ["Hello from Java"]);
    }

    #[test]
    fn enforces_fuel_for_fabric_entrypoints() {
        let jar = tiny_fabric_mod_jar();
        assert!(matches!(
            run_fabric_mod_jar(&jar, 0),
            Err(ModExecutionError::Vm(VmError::FuelExhausted))
        ));
    }

    #[test]
    fn executes_integer_arithmetic_and_return() {
        // (6 * 7) - 2
        let code = [0x10, 0x06, 0x10, 0x07, 0x68, 0x05, 0x64, 0xac];
        assert_eq!(Vm::default().execute_static_int(&code, &[]), Ok(40));
    }

    #[test]
    fn executes_locals_and_iinc() {
        // local[0] += 5; return local[0]
        let code = [0x84, 0x00, 0x05, 0x1a, 0xac];
        assert_eq!(Vm::default().execute_static_int(&code, &[37]), Ok(42));
    }

    #[test]
    fn executes_conditional_branch() {
        // iconst_0; ifeq +5; iconst_1; ireturn; bipush 42; ireturn
        let code = [0x03, 0x99, 0x00, 0x05, 0x04, 0xac, 0x10, 0x2a, 0xac];
        assert_eq!(Vm::default().execute_static_int(&code, &[]), Ok(42));
    }

    #[test]
    fn division_by_zero_is_reported() {
        assert_eq!(
            Vm::default().execute_static_int(&[0x04, 0x03, 0x6c, 0xac], &[]),
            Err(VmError::DivisionByZero)
        );
    }

    #[test]
    fn fuel_bounds_infinite_loops() {
        let vm = Vm::new(ExecutionLimits {
            fuel: 4,
            ..ExecutionLimits::default()
        });
        assert_eq!(
            vm.execute(&[0xa7, 0x00, 0x00], &[]),
            Err(VmError::FuelExhausted)
        );
    }

    #[test]
    fn rejects_unknown_opcodes() {
        assert!(matches!(
            Vm::default().execute(&[0x00], &[]),
            Err(VmError::UnsupportedOpcode(0x00, 0))
        ));
    }
}
