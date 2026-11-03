//! Pomme's Rust-implemented JVM runtime.
//!
//! This crate starts with a fuel-bounded interpreter for a well-defined JVM
//! integer-bytecode subset. It is deliberately not advertised as a complete
//! JVM: class loading, verification, object semantics, exceptions, threads,
//! the Java class library, and dynamic linking are subsequent milestones.

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
        Self { max_stack: 1024, max_locals: 256, fuel: 1_000_000 }
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
    pub fn new(limits: ExecutionLimits) -> Self { Self { limits } }

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
            if fuel == 0 { return Err(VmError::FuelExhausted); }
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
                    let bytes = code.get(pc..pc + 2).ok_or(VmError::TruncatedInstruction(op_pc))?;
                    pc += 2;
                    push(&mut stack, i16::from_be_bytes([bytes[0], bytes[1]]) as i32, self.limits.max_stack)?;
                }
                0x15 | 0x36 => {
                    let index = *code.get(pc).ok_or(VmError::TruncatedInstruction(op_pc))? as usize;
                    pc += 1;
                    check_local(index, self.limits.max_locals)?;
                    if op == 0x15 { push(&mut stack, locals[index], self.limits.max_stack)?; }
                    else { locals[index] = pop(&mut stack)?; }
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
                0x57 => { pop(&mut stack)?; }
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
                    if rhs == 0 { return Err(VmError::DivisionByZero); }
                    let value = if lhs == i32::MIN && rhs == -1 { i32::MIN } else { lhs / rhs };
                    push(&mut stack, value, self.limits.max_stack)?;
                }
                0x70 => {
                    let rhs = pop(&mut stack)?;
                    let lhs = pop(&mut stack)?;
                    if rhs == 0 { return Err(VmError::DivisionByZero); }
                    push(&mut stack, if lhs == i32::MIN && rhs == -1 { 0 } else { lhs % rhs }, self.limits.max_stack)?;
                }
                0x74 => {
                    let value = pop(&mut stack)?;
                    push(&mut stack, value.wrapping_neg(), self.limits.max_stack)?;
                }
                0x84 => {
                    let bytes = code.get(pc..pc + 2).ok_or(VmError::TruncatedInstruction(op_pc))?;
                    pc += 2;
                    let index = bytes[0] as usize;
                    check_local(index, self.limits.max_locals)?;
                    locals[index] = locals[index].wrapping_add(bytes[1] as i8 as i32);
                }
                0x99..=0x9e | 0xa7 => {
                    let bytes = code.get(pc..pc + 2).ok_or(VmError::TruncatedInstruction(op_pc))?;
                    pc += 2;
                    let offset = i16::from_be_bytes([bytes[0], bytes[1]]) as isize;
                    let take = if op == 0xa7 { true } else {
                        let value = pop(&mut stack)?;
                        match op {
                            0x99 => value == 0, 0x9a => value != 0, 0x9b => value < 0,
                            0x9c => value >= 0, 0x9d => value > 0, 0x9e => value <= 0,
                            _ => unreachable!(),
                        }
                    };
                    if take { pc = branch_target(op_pc, offset, code.len())?; }
                }
                0xac => return Ok(ReturnValue::Int(pop(&mut stack)?)),
                0xb1 => {
                    if !stack.is_empty() { return Err(VmError::InvalidReturnState); }
                    return Ok(ReturnValue::Void);
                }
                other => return Err(VmError::UnsupportedOpcode(other, op_pc)),
            }
        }
        Err(VmError::MissingReturn)
    }
}

impl Default for Vm {
    fn default() -> Self { Self::new(ExecutionLimits::default()) }
}

fn push(stack: &mut Vec<i32>, value: i32, limit: usize) -> Result<(), VmError> {
    if stack.len() >= limit { return Err(VmError::StackOverflow); }
    stack.push(value);
    Ok(())
}
fn pop(stack: &mut Vec<i32>) -> Result<i32, VmError> { stack.pop().ok_or(VmError::StackUnderflow) }
fn check_local(index: usize, limit: usize) -> Result<(), VmError> {
    if index >= limit { Err(VmError::InvalidLocal(index)) } else { Ok(()) }
}
fn binary(stack: &mut Vec<i32>, limit: usize, f: fn(i32, i32) -> i32) -> Result<(), VmError> {
    let rhs = pop(stack)?;
    let lhs = pop(stack)?;
    push(stack, f(lhs, rhs), limit)
}
fn branch_target(origin: usize, offset: isize, code_len: usize) -> Result<usize, VmError> {
    let target = origin as isize + offset;
    if target < 0 || target as usize >= code_len { return Err(VmError::InvalidBranchTarget(target)); }
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
    #[error("fabric.mod.json has no main entrypoints")]
    NoMainEntrypoints,
    #[error("unsupported Fabric entrypoint value (expected a class-name string)")]
    InvalidEntrypoint,
    #[error("entrypoint class {0} is missing from the JAR")]
    MissingClass(String),
    #[error("class-file analysis failed: {0}")]
    ClassFile(#[from] pomme_mod::ClassFileError),
    #[error("class {0} is outside the supported Java class-file versions (45..=65)")]
    UnsupportedClassVersion(String),
    #[error("class {0} must have no instance fields or static initializer yet")]
    UnsupportedClassShape(String),
    #[error("class {0} must have a trivial public no-argument constructor calling only Object.<init>")]
    UnsupportedConstructor(String),
    #[error("class {0} must implement a public instance onInitialize()V method")]
    MissingInitializer(String),
    #[error("class {0} onInitialize()V contains unsupported bytecode or API calls")]
    UnsupportedInitializer(String),
    #[error("JVM execution failed: {0}")]
    Vm(#[from] VmError),
}

#[derive(serde::Deserialize)]
struct FabricModMetadata {
    #[serde(rename = "schemaVersion")]
    schema_version: u32,
    id: String,
    #[serde(default)]
    entrypoints: std::collections::BTreeMap<String, Vec<serde_json::Value>>,
}

/// Load and execute the first supported Java Fabric mod from a JAR.
///
/// This milestone intentionally supports the smallest useful Fabric shape:
/// a Java 21-or-earlier class with no instance fields or static initializer,
/// a trivial constructor that only invokes Object.<init>, and a public
/// instance onInitialize()V containing string constants, Pomme.log bridge
/// calls, and return. It is not general Fabric Loader compatibility.
pub fn run_fabric_mod_jar(bytes: &[u8], fuel_per_entrypoint: u64) -> Result<ModRunReport, ModExecutionError> {
    use std::io::Read;
    use zip::ZipArchive;

    let mut archive = ZipArchive::new(std::io::Cursor::new(bytes))?;
    if archive.len() > 10_000 {
        return Err(ModExecutionError::InvalidMetadata);
    }

    let metadata = {
        let file = archive.by_name("fabric.mod.json")
            .map_err(|_| ModExecutionError::MissingFabricMetadata)?;
        let mut raw = Vec::new();
        file.take(1024 * 1024 + 1).read_to_end(&mut raw)?;
        if raw.len() > 1024 * 1024 {
            return Err(ModExecutionError::InvalidMetadata);
        }
        serde_json::from_slice::<FabricModMetadata>(&raw)?
    };
    let valid_id = !metadata.id.is_empty()
        && metadata.id.len() <= 64
        && metadata.id.as_bytes()[0].is_ascii_alphabetic()
        && metadata.id.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-');
    if metadata.schema_version != 1 || !valid_id {
        return Err(ModExecutionError::InvalidMetadata);
    }
    let main = metadata.entrypoints.get("main").ok_or(ModExecutionError::NoMainEntrypoints)?;
    if main.is_empty() || main.len() > 256 {
        return Err(ModExecutionError::NoMainEntrypoints);
    }

    let mut initialized_classes = Vec::with_capacity(main.len());
    let mut instructions_executed = 0u64;
    let mut logs = Vec::new();
    for value in main {
        let class_name = value.as_str()
            .or_else(|| value.get("value").and_then(serde_json::Value::as_str))
            .ok_or(ModExecutionError::InvalidEntrypoint)?;
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
        let path = format!("{}.class", class_name.replace('.', "/"));
        let class_bytes = {
            let file = archive.by_name(&path)
                .map_err(|_| ModExecutionError::MissingClass(class_name.to_owned()))?;
            let mut raw = Vec::new();
            file.take(32 * 1024 * 1024 + 1).read_to_end(&mut raw)?;
            if raw.len() > 32 * 1024 * 1024 {
                return Err(ModExecutionError::InvalidMetadata);
            }
            raw
        };
        let (executed, mut class_logs) =
            execute_minimal_fabric_entrypoint(&class_bytes, class_name, fuel_per_entrypoint)?;
        instructions_executed = instructions_executed.saturating_add(executed);
        logs.append(&mut class_logs);
        initialized_classes.push(class_name.to_owned());
    }

    Ok(ModRunReport {
        mod_id: metadata.id,
        initialized_classes,
        instructions_executed,
        logs,
    })
}

fn execute_minimal_fabric_entrypoint(
    class_bytes: &[u8],
    display_name: &str,
    fuel: u64,
) -> Result<(u64, Vec<String>), ModExecutionError> {
    let class = pomme_mod::analyze_class(class_bytes)?;
    if !(45..=65).contains(&class.major_version) {
        return Err(ModExecutionError::UnsupportedClassVersion(display_name.to_owned()));
    }
    if class.fields != 0
        || class.has_static_initializer
        || class.access_flags & 0x0001 == 0
        || !class.interfaces.iter().any(|name| name == "net/fabricmc/api/ModInitializer")
    {
        return Err(ModExecutionError::UnsupportedClassShape(display_name.to_owned()));
    }

    let constructor = class.methods.iter()
        .find(|method| method.name == "<init>" && method.descriptor == "()V")
        .ok_or_else(|| ModExecutionError::UnsupportedConstructor(display_name.to_owned()))?;
    let valid_constructor = constructor.access_flags & 0x0001 != 0
        && constructor.code.len() == 5
        && constructor.code[0] == 0x2a
        && constructor.code[1] == 0xb7
        && constructor.code[4] == 0xb1
        && constructor.invoked_methods.len() == 1
        && constructor.invoked_methods[0].owner == "java/lang/Object"
        && constructor.invoked_methods[0].name == "<init>"
        && constructor.invoked_methods[0].descriptor == "()V";
    if !valid_constructor {
        return Err(ModExecutionError::UnsupportedConstructor(display_name.to_owned()));
    }

    let initializer = class.methods.iter()
        .find(|method| method.name == "onInitialize" && method.descriptor == "()V")
        .ok_or_else(|| ModExecutionError::MissingInitializer(display_name.to_owned()))?;
    if initializer.access_flags & 0x0001 == 0 || initializer.access_flags & 0x0008 != 0 {
        return Err(ModExecutionError::MissingInitializer(display_name.to_owned()));
    }

    // First object-model milestone: the receiver is represented by a checked
    // placeholder local. The supported method body cannot inspect or mutate
    // object fields; it can emit messages through the Pomme Java bridge.
    let code = &initializer.code;
    let mut pc = 0usize;
    let mut fuel_left = fuel;
    let mut instructions = 0u64;
    let mut stack: Vec<String> = Vec::with_capacity(usize::from(initializer.max_stack));
    let mut logs = Vec::new();
    loop {
        if fuel_left == 0 {
            return Err(ModExecutionError::Vm(VmError::FuelExhausted));
        }
        fuel_left -= 1;
        instructions += 1;
        let opcode = *code.get(pc).ok_or_else(|| ModExecutionError::UnsupportedInitializer(display_name.to_owned()))?;
        pc += 1;
        match opcode {
            0x00 => {}
            0x12 => {
                let index = *code.get(pc).ok_or_else(|| ModExecutionError::UnsupportedInitializer(display_name.to_owned()))? as u16;
                pc += 1;
                let value = class.string_constants.get(&index)
                    .ok_or_else(|| ModExecutionError::UnsupportedInitializer(display_name.to_owned()))?;
                if stack.len() >= usize::from(initializer.max_stack) {
                    return Err(ModExecutionError::UnsupportedInitializer(display_name.to_owned()));
                }
                stack.push(value.clone());
            }
            0x13 => {
                let bytes = code.get(pc..pc + 2)
                    .ok_or_else(|| ModExecutionError::UnsupportedInitializer(display_name.to_owned()))?;
                pc += 2;
                let index = u16::from_be_bytes([bytes[0], bytes[1]]);
                let value = class.string_constants.get(&index)
                    .ok_or_else(|| ModExecutionError::UnsupportedInitializer(display_name.to_owned()))?;
                if stack.len() >= usize::from(initializer.max_stack) {
                    return Err(ModExecutionError::UnsupportedInitializer(display_name.to_owned()));
                }
                stack.push(value.clone());
            }
            0xb8 => {
                let bytes = code.get(pc..pc + 2)
                    .ok_or_else(|| ModExecutionError::UnsupportedInitializer(display_name.to_owned()))?;
                pc += 2;
                let index = u16::from_be_bytes([bytes[0], bytes[1]]);
                let method = class.method_references.get(&index)
                    .ok_or_else(|| ModExecutionError::UnsupportedInitializer(display_name.to_owned()))?;
                if method.owner != "pomme/api/Pomme"
                    || method.name != "log"
                    || method.descriptor != "(Ljava/lang/String;)V"
                {
                    return Err(ModExecutionError::UnsupportedInitializer(display_name.to_owned()));
                }
                logs.push(stack.pop().ok_or_else(|| ModExecutionError::UnsupportedInitializer(display_name.to_owned()))?);
            }
            0xb1 => {
                if !stack.is_empty() {
                    return Err(ModExecutionError::UnsupportedInitializer(display_name.to_owned()));
                }
                return Ok((instructions, logs));
            }
            _ => return Err(ModExecutionError::UnsupportedInitializer(display_name.to_owned())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;


    fn tiny_fabric_mod_jar() -> Vec<u8> {
        use std::io::Write;
        use zip::write::SimpleFileOptions;

        fn u2(out: &mut Vec<u8>, value: u16) { out.extend(value.to_be_bytes()); }
        fn u4(out: &mut Vec<u8>, value: u32) { out.extend(value.to_be_bytes()); }
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
        u2(&mut class, 21);
        utf8(&mut class, "example/ExampleMod"); // #1
        class.push(7); u2(&mut class, 1);       // #2 Class
        utf8(&mut class, "java/lang/Object");   // #3
        class.push(7); u2(&mut class, 3);       // #4 Class
        utf8(&mut class, "<init>");             // #5
        utf8(&mut class, "()V");                // #6
        class.push(12); u2(&mut class, 5); u2(&mut class, 6); // #7 NameAndType
        class.push(10); u2(&mut class, 4); u2(&mut class, 7); // #8 Methodref
        utf8(&mut class, "Code");               // #9
        utf8(&mut class, "onInitialize");       // #10
        utf8(&mut class, "pomme/api/Pomme");     // #11
        class.push(7); u2(&mut class, 11);       // #12 Class
        utf8(&mut class, "log");                 // #13
        utf8(&mut class, "(Ljava/lang/String;)V"); // #14
        class.push(12); u2(&mut class, 13); u2(&mut class, 14); // #15 NameAndType
        class.push(10); u2(&mut class, 12); u2(&mut class, 15); // #16 Methodref
        utf8(&mut class, "Hello from Java");     // #17
        class.push(8); u2(&mut class, 17);       // #18 String
        utf8(&mut class, "net/fabricmc/api/ModInitializer"); // #19
        class.push(7); u2(&mut class, 19);       // #20 Class

        u2(&mut class, 0x0021); // public super
        u2(&mut class, 2);     // this
        u2(&mut class, 4);     // super
        u2(&mut class, 1);     // interfaces
        u2(&mut class, 20);    // ModInitializer
        u2(&mut class, 0);     // fields
        u2(&mut class, 2);     // methods

        // Public no-arg constructor: aload_0; invokespecial Object.<init>; return.
        u2(&mut class, 0x0001); u2(&mut class, 5); u2(&mut class, 6); u2(&mut class, 1);
        u2(&mut class, 9); u4(&mut class, 17);
        u2(&mut class, 1); u2(&mut class, 1); u4(&mut class, 5);
        class.extend([0x2a, 0xb7, 0x00, 0x08, 0xb1]);
        u2(&mut class, 0); u2(&mut class, 0);

        // Public instance initializer: ldc "Hello from Java"; invokestatic Pomme.log; return.
        u2(&mut class, 0x0001); u2(&mut class, 10); u2(&mut class, 6); u2(&mut class, 1);
        u2(&mut class, 9); u4(&mut class, 18);
        u2(&mut class, 1); u2(&mut class, 1); u4(&mut class, 6);
        class.extend([0x12, 0x12, 0xb8, 0x00, 0x10, 0xb1]);
        u2(&mut class, 0); u2(&mut class, 0);
        u2(&mut class, 0); // class attributes

        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        writer.start_file("fabric.mod.json", SimpleFileOptions::default()).unwrap();
        writer.write_all(br#"{"schemaVersion":1,"id":"tiny_example","version":"1.0.0","entrypoints":{"main":["example.ExampleMod"]}}"#).unwrap();
        writer.start_file("example/ExampleMod.class", SimpleFileOptions::default()).unwrap();
        writer.write_all(&class).unwrap();
        writer.finish().unwrap().into_inner()
    }

    #[test]
    fn loads_a_fabric_jar_and_executes_a_java_on_initialize_method() {
        let jar = tiny_fabric_mod_jar();
        let report = run_fabric_mod_jar(&jar, 100).unwrap();
        assert_eq!(report.mod_id, "tiny_example");
        assert_eq!(report.initialized_classes, ["example.ExampleMod"]);
        assert_eq!(report.instructions_executed, 3);
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
        assert_eq!(Vm::default().execute_static_int(&[0x04, 0x03, 0x6c, 0xac], &[]), Err(VmError::DivisionByZero));
    }

    #[test]
    fn fuel_bounds_infinite_loops() {
        let vm = Vm::new(ExecutionLimits { fuel: 4, ..ExecutionLimits::default() });
        assert_eq!(vm.execute(&[0xa7, 0x00, 0x00], &[]), Err(VmError::FuelExhausted));
    }

    #[test]
    fn rejects_unknown_opcodes() {
        assert!(matches!(Vm::default().execute(&[0x00], &[]), Err(VmError::UnsupportedOpcode(0x00, 0))));
    }
}
