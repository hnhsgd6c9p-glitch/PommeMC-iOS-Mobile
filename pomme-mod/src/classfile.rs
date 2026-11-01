//! Defensive Java class-file reader and bytecode feature inventory.
//!
//! This is an analyzer, not a JVM: it never executes class initializers or
//! bytecode. Unknown attributes are skipped using their declared lengths.

use std::collections::{BTreeMap, BTreeSet};

use thiserror::Error;

const MAX_CLASS_BYTES: usize = 32 * 1024 * 1024;
const MAX_ITEMS: usize = 1_000_000;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ClassFileError {
    #[error("class file exceeds the {MAX_CLASS_BYTES}-byte limit")]
    TooLarge,
    #[error("invalid class-file magic")]
    BadMagic,
    #[error("unsupported constant-pool tag {0}")]
    UnknownConstantTag(u8),
    #[error("truncated or malformed class file")]
    Truncated,
    #[error("invalid constant-pool reference at index {0}")]
    BadConstant(u16),
    #[error("invalid bytecode instruction {0:#04x}")]
    InvalidOpcode(u8),
    #[error("invalid bytecode instruction layout")]
    InvalidBytecode,
    #[error("class file contains too many entries")]
    TooManyItems,
    #[error("invalid modified UTF-8 constant")]
    InvalidUtf8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MethodReference {
    pub owner: String,
    pub name: String,
    pub descriptor: String,
    pub interface_call: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MethodSummary {
    pub name: String,
    pub descriptor: String,
    pub access_flags: u16,
    pub bytecode_bytes: usize,
    pub instruction_count: usize,
    pub max_stack: u16,
    pub max_locals: u16,
    /// Raw JVM Code attribute instructions, retained for the restricted Rust
    /// interpreter.
    pub code: Vec<u8>,
    pub invoked_methods: Vec<MethodReference>,
    pub field_accesses: Vec<FieldReference>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldReference {
    pub owner: String,
    pub name: String,
    pub descriptor: String,
    pub opcode: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassAnalysis {
    pub minor_version: u16,
    pub major_version: u16,
    pub access_flags: u16,
    pub name: String,
    pub super_name: Option<String>,
    pub interfaces: Vec<String>,
    pub fields: usize,
    pub methods: Vec<MethodSummary>,
    pub annotations: usize,
    pub has_static_initializer: bool,
    pub uses_reflection: bool,
    pub uses_native_methods: bool,
    pub opcode_counts: BTreeMap<String, usize>,
    pub referenced_classes: BTreeSet<String>,
    pub invoked_methods: BTreeSet<String>,
    pub field_references: BTreeSet<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct JarCompatibilityReport {
    pub analyzed_classes: usize,
    pub skipped_classes: usize,
    pub class_versions: BTreeMap<u16, usize>,
    pub opcode_counts: BTreeMap<String, usize>,
    pub referenced_classes: BTreeSet<String>,
    pub invoked_methods: BTreeSet<String>,
    pub field_references: BTreeSet<String>,
    pub feature_flags: BTreeSet<String>,
    pub unsupported_or_high_risk_apis: BTreeSet<String>,
}

#[derive(Clone, Debug)]
enum CpEntry {
    Empty,
    Utf8(String),
    Class(u16),
    NameAndType(u16, u16),
    Ref {
        class: u16,
        name_type: u16,
        interface: bool,
        field: bool,
    },
    Other,
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}
impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8], ClassFileError> {
        let end = self.pos.checked_add(n).ok_or(ClassFileError::Truncated)?;
        let value = self
            .bytes
            .get(self.pos..end)
            .ok_or(ClassFileError::Truncated)?;
        self.pos = end;
        Ok(value)
    }
    fn u1(&mut self) -> Result<u8, ClassFileError> {
        Ok(self.take(1)?[0])
    }
    fn u2(&mut self) -> Result<u16, ClassFileError> {
        Ok(u16::from_be_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u4(&mut self) -> Result<u32, ClassFileError> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn skip(&mut self, n: usize) -> Result<(), ClassFileError> {
        self.take(n).map(|_| ())
    }
    fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.pos)
    }
}

pub fn analyze_class(bytes: &[u8]) -> Result<ClassAnalysis, ClassFileError> {
    if bytes.len() > MAX_CLASS_BYTES {
        return Err(ClassFileError::TooLarge);
    }
    let mut r = Reader::new(bytes);
    if r.u4()? != 0xCAFEBABE {
        return Err(ClassFileError::BadMagic);
    }
    let minor_version = r.u2()?;
    let major_version = r.u2()?;
    let cp_count = bounded(r.u2()? as usize)?;
    let mut cp = Vec::with_capacity(cp_count);
    cp.push(CpEntry::Empty);
    let mut i = 1;
    while i < cp_count {
        let tag = r.u1()?;
        let entry = match tag {
            1 => {
                let len = r.u2()? as usize;
                CpEntry::Utf8(decode_modified_utf8(r.take(len)?)?)
            }
            7 => CpEntry::Class(r.u2()?),
            9..=11 => CpEntry::Ref {
                class: r.u2()?,
                name_type: r.u2()?,
                interface: tag == 11,
                field: tag == 9,
            },
            12 => CpEntry::NameAndType(r.u2()?, r.u2()?),
            3 | 4 => {
                r.skip(4)?;
                CpEntry::Other
            }
            5 | 6 => {
                r.skip(8)?;
                cp.push(CpEntry::Other);
                cp.push(CpEntry::Empty);
                i += 1;
                i += 1;
                continue;
            }
            8 | 16 | 19 | 20 => {
                r.skip(2)?;
                CpEntry::Other
            }
            15 => {
                r.skip(3)?;
                CpEntry::Other
            }
            17 | 18 => {
                r.skip(4)?;
                CpEntry::Other
            }
            other => return Err(ClassFileError::UnknownConstantTag(other)),
        };
        cp.push(entry);
        i += 1;
    }
    let access_flags = r.u2()?;
    let this_class = r.u2()?;
    let super_class = r.u2()?;
    let name = class_name(&cp, this_class)?;
    let super_name = if super_class == 0 {
        None
    } else {
        Some(class_name(&cp, super_class)?)
    };
    let interface_count = bounded(r.u2()? as usize)?;
    let mut interfaces = Vec::with_capacity(interface_count);
    for _ in 0..interface_count {
        interfaces.push(class_name(&cp, r.u2()?)?);
    }
    let field_count = bounded(r.u2()? as usize)?;
    for _ in 0..field_count {
        skip_member(&mut r)?;
    }
    let method_count = bounded(r.u2()? as usize)?;
    let mut methods = Vec::with_capacity(method_count);
    let mut opcode_counts = BTreeMap::new();
    let mut referenced_classes = BTreeSet::new();
    let mut invoked_methods = BTreeSet::new();
    let mut field_references = BTreeSet::new();
    let mut annotations = 0;
    let mut has_static_initializer = false;
    let mut uses_native_methods = false;
    let mut uses_reflection = false;
    for _ in 0..method_count {
        let flags = r.u2()?;
        let method_name = utf8(&cp, r.u2()?)?;
        let descriptor = utf8(&cp, r.u2()?)?;
        let attr_count = bounded(r.u2()? as usize)?;
        let mut bytecode_bytes = 0;
        let mut instruction_count = 0;
        let mut max_stack = 0;
        let mut max_locals = 0;
        let mut method_code = Vec::new();
        let mut method_invokes = Vec::new();
        let mut method_field_accesses = Vec::new();
        if method_name == "<clinit>" {
            has_static_initializer = true;
        }
        if flags & 0x0100 != 0 {
            uses_native_methods = true;
        }
        for _ in 0..attr_count {
            let attr_name = utf8(&cp, r.u2()?)?;
            let len = bounded(r.u4()? as usize)?;
            let data = r.take(len)?;
            if attr_name == "Code" {
                let mut code = Reader::new(data);
                max_stack = code.u2()?;
                max_locals = code.u2()?;
                let code_len = bounded(code.u4()? as usize)?;
                let instructions = code.take(code_len)?;
                bytecode_bytes = code_len;
                method_code.extend_from_slice(instructions);
                let (counts, calls, fields, count) = decode_bytecode(instructions, &cp)?;
                instruction_count = count;
                for (op, n) in counts {
                    *opcode_counts.entry(op).or_insert(0) += n;
                }
                for call in calls {
                    let key = format!("{}.{}{}", call.owner, call.name, call.descriptor);
                    if key.contains("java/lang/reflect/")
                        || key.contains("java/lang/invoke/MethodHandle")
                    {
                        uses_reflection = true;
                    }
                    invoked_methods.insert(key);
                    method_invokes.push(call);
                }
                for field in fields {
                    let key = format!(
                        "{}.{}:{} [{}]",
                        field.owner, field.name, field.descriptor, field.opcode
                    );
                    field_references.insert(key);
                    method_field_accesses.push(field);
                }
                let ex_count = bounded(code.u2()? as usize)?;
                code.skip(
                    ex_count
                        .checked_mul(8)
                        .ok_or(ClassFileError::TooManyItems)?,
                )?;
                skip_attributes(&mut code, &cp, &mut annotations)?;
                if code.remaining() != 0 {
                    return Err(ClassFileError::InvalidBytecode);
                }
            } else {
                count_annotation_attribute(&attr_name, data, &cp, &mut annotations)?;
            }
        }
        methods.push(MethodSummary {
            name: method_name,
            descriptor,
            access_flags: flags,
            bytecode_bytes,
            instruction_count,
            max_stack,
            max_locals,
            code: method_code,
            invoked_methods: method_invokes,
            field_accesses: method_field_accesses,
        });
    }
    skip_attributes(&mut r, &cp, &mut annotations)?;
    // Class references are collected from constant-pool Class entries and method
    // owners.
    for (index, entry) in cp.iter().enumerate() {
        if matches!(entry, CpEntry::Class(_))
            && let Ok(class) = class_name(&cp, index as u16)
        {
            referenced_classes.insert(class);
        }
    }
    for method in &methods {
        for call in &method.invoked_methods {
            referenced_classes.insert(call.owner.clone());
        }
    }
    Ok(ClassAnalysis {
        minor_version,
        major_version,
        access_flags,
        name,
        super_name,
        interfaces,
        fields: field_count,
        methods,
        annotations,
        has_static_initializer,
        uses_reflection,
        uses_native_methods,
        opcode_counts,
        referenced_classes,
        invoked_methods,
        field_references,
    })
}

pub fn analyze_jar_classes<I, N>(classes: I) -> JarCompatibilityReport
where
    I: IntoIterator<Item = (N, Vec<u8>)>,
    N: AsRef<str>,
{
    let mut report = JarCompatibilityReport::default();
    for (path, bytes) in classes {
        match analyze_class(&bytes) {
            Ok(class) => {
                report.analyzed_classes += 1;
                *report
                    .class_versions
                    .entry(class.major_version)
                    .or_insert(0) += 1;
                for (op, n) in class.opcode_counts {
                    *report.opcode_counts.entry(op).or_insert(0) += n;
                }
                report.referenced_classes.extend(class.referenced_classes);
                report.invoked_methods.extend(class.invoked_methods);
                report.field_references.extend(class.field_references);
                if class.has_static_initializer {
                    report.feature_flags.insert("static_initializers".into());
                }
                if class.uses_reflection {
                    report
                        .feature_flags
                        .insert("reflection_or_method_handles".into());
                }
                if class.uses_native_methods {
                    report.feature_flags.insert("native_methods".into());
                }
                if class.annotations > 0 {
                    report.feature_flags.insert("annotations".into());
                }
                let _ = path;
            }
            Err(_) => report.skipped_classes += 1,
        }
    }
    for method in &report.invoked_methods {
        if method.starts_with("java/lang/reflect/") || method.starts_with("java/lang/invoke/") {
            report.unsupported_or_high_risk_apis.insert(method.clone());
        }
        if method.starts_with("java/lang/Thread.") || method.starts_with("java/util/concurrent/") {
            report
                .feature_flags
                .insert("threading_or_concurrency".into());
        }
        if method.starts_with("java/io/") || method.starts_with("java/nio/file/") {
            report.feature_flags.insert("filesystem_api_usage".into());
        }
        if method.starts_with("java/net/") {
            report.feature_flags.insert("network_api_usage".into());
        }
        if method.starts_with("java/lang/Class.") || method.starts_with("java/lang/ClassLoader.") {
            report.feature_flags.insert("dynamic_class_loading".into());
        }
        if method.starts_with("sun/") || method.starts_with("jdk/internal/") {
            report.unsupported_or_high_risk_apis.insert(method.clone());
        }
    }
    report
}

fn bounded(value: usize) -> Result<usize, ClassFileError> {
    if value > MAX_ITEMS {
        Err(ClassFileError::TooManyItems)
    } else {
        Ok(value)
    }
}
fn utf8(cp: &[CpEntry], index: u16) -> Result<String, ClassFileError> {
    match cp.get(index as usize) {
        Some(CpEntry::Utf8(s)) => Ok(s.clone()),
        _ => Err(ClassFileError::BadConstant(index)),
    }
}
fn class_name(cp: &[CpEntry], index: u16) -> Result<String, ClassFileError> {
    match cp.get(index as usize) {
        Some(CpEntry::Class(name)) => utf8(cp, *name),
        _ => Err(ClassFileError::BadConstant(index)),
    }
}
fn name_and_type(cp: &[CpEntry], index: u16) -> Result<(String, String), ClassFileError> {
    match cp.get(index as usize) {
        Some(CpEntry::NameAndType(n, d)) => Ok((utf8(cp, *n)?, utf8(cp, *d)?)),
        _ => Err(ClassFileError::BadConstant(index)),
    }
}
fn method_ref(cp: &[CpEntry], index: u16) -> Result<MethodReference, ClassFileError> {
    match cp.get(index as usize) {
        Some(CpEntry::Ref {
            class,
            name_type,
            interface,
            field: false,
        }) => {
            let (name, descriptor) = name_and_type(cp, *name_type)?;
            Ok(MethodReference {
                owner: class_name(cp, *class)?,
                name,
                descriptor,
                interface_call: *interface,
            })
        }
        _ => Err(ClassFileError::BadConstant(index)),
    }
}
fn field_ref(cp: &[CpEntry], index: u16, opcode: &str) -> Result<FieldReference, ClassFileError> {
    match cp.get(index as usize) {
        Some(CpEntry::Ref {
            class,
            name_type,
            field: true,
            ..
        }) => {
            let (name, descriptor) = name_and_type(cp, *name_type)?;
            Ok(FieldReference {
                owner: class_name(cp, *class)?,
                name,
                descriptor,
                opcode: opcode.to_owned(),
            })
        }
        _ => Err(ClassFileError::BadConstant(index)),
    }
}
fn skip_member(r: &mut Reader<'_>) -> Result<(), ClassFileError> {
    r.skip(6)?;
    let count = bounded(r.u2()? as usize)?;
    for _ in 0..count {
        r.skip(2)?;
        let len = bounded(r.u4()? as usize)?;
        r.skip(len)?;
    }
    Ok(())
}
fn skip_attributes(
    r: &mut Reader<'_>,
    _cp: &[CpEntry],
    annotations: &mut usize,
) -> Result<(), ClassFileError> {
    let count = bounded(r.u2()? as usize)?;
    for _ in 0..count {
        let _name = r.u2()?;
        let len = bounded(r.u4()? as usize)?;
        r.skip(len)?;
    }
    let _ = annotations;
    Ok(())
}
fn count_annotation_attribute(
    name: &str,
    data: &[u8],
    _cp: &[CpEntry],
    count: &mut usize,
) -> Result<(), ClassFileError> {
    if name == "RuntimeVisibleAnnotations" || name == "RuntimeInvisibleAnnotations" {
        let mut r = Reader::new(data);
        *count += r.u2()? as usize;
    }
    Ok(())
}

type DecodedBytecode = (
    BTreeMap<String, usize>,
    Vec<MethodReference>,
    Vec<FieldReference>,
    usize,
);

fn decode_bytecode(code: &[u8], cp: &[CpEntry]) -> Result<DecodedBytecode, ClassFileError> {
    let mut counts = BTreeMap::new();
    let mut calls = Vec::new();
    let mut fields = Vec::new();
    let mut pc = 0usize;
    let mut instructions = 0;
    while pc < code.len() {
        let op = code[pc];
        let start = pc;
        pc += 1;
        instructions += 1;
        let name = opcode_name(op).ok_or(ClassFileError::InvalidOpcode(op))?;
        *counts.entry(name.to_string()).or_insert(0) += 1;
        match op {
            0x10 | 0x12 | 0x15..=0x19 | 0x36..=0x3a | 0xa9 | 0xbc => pc += 1,
            0x11 | 0x13 | 0x14 | 0x84 | 0x99..=0xa8 | 0xc6 | 0xc7 => pc += 2,
            0xb2..=0xb8 | 0xbb | 0xbd | 0xc0 | 0xc1 => {
                let index = read_u16(code, pc)?;
                pc += 2;
                match op {
                    0xb2..=0xb5 => fields.push(field_ref(cp, index, name)?),
                    0xb6..=0xb8 => calls.push(method_ref(cp, index)?),
                    _ => {}
                }
            }
            0xb9 => {
                let index = read_u16(code, pc)?;
                pc += 4;
                calls.push(method_ref(cp, index)?);
            }
            0xba => pc += 4,
            0xc5 => pc += 3,
            0xc8 | 0xc9 => pc += 4,
            0xaa => {
                while !pc.is_multiple_of(4) {
                    pc += 1;
                }
                let _default = read_i32(code, pc)?;
                let low = read_i32(code, pc + 4)?;
                let high = read_i32(code, pc + 8)?;
                if high < low {
                    return Err(ClassFileError::InvalidBytecode);
                }
                let n = (high as i64 - low as i64 + 1) as usize;
                if n > MAX_ITEMS {
                    return Err(ClassFileError::TooManyItems);
                }
                pc += 12 + n.checked_mul(4).ok_or(ClassFileError::TooManyItems)?;
            }
            0xab => {
                while !pc.is_multiple_of(4) {
                    pc += 1;
                }
                let _default = read_i32(code, pc)?;
                let n = read_i32(code, pc + 4)?;
                if n < 0 || n as usize > MAX_ITEMS {
                    return Err(ClassFileError::InvalidBytecode);
                }
                pc += 8
                    + (n as usize)
                        .checked_mul(8)
                        .ok_or(ClassFileError::TooManyItems)?;
            }
            0xc4 => {
                let next = *code.get(pc).ok_or(ClassFileError::InvalidBytecode)?;
                pc += if next == 0x84 {
                    5
                } else if matches!(next, 0x15..=0x19 | 0x36..=0x3a | 0xa9) {
                    3
                } else {
                    return Err(ClassFileError::InvalidBytecode);
                };
            }
            _ => {}
        }
        if pc > code.len() || pc <= start {
            return Err(ClassFileError::InvalidBytecode);
        }
    }
    Ok((counts, calls, fields, instructions))
}
fn read_u16(b: &[u8], p: usize) -> Result<u16, ClassFileError> {
    Ok(u16::from_be_bytes(
        b.get(p..p + 2)
            .ok_or(ClassFileError::InvalidBytecode)?
            .try_into()
            .unwrap(),
    ))
}
fn read_i32(b: &[u8], p: usize) -> Result<i32, ClassFileError> {
    Ok(i32::from_be_bytes(
        b.get(p..p + 4)
            .ok_or(ClassFileError::InvalidBytecode)?
            .try_into()
            .unwrap(),
    ))
}

fn opcode_name(op: u8) -> Option<&'static str> {
    const N: [&str; 0xCA] = [
        "nop",
        "aconst_null",
        "iconst_m1",
        "iconst_0",
        "iconst_1",
        "iconst_2",
        "iconst_3",
        "iconst_4",
        "iconst_5",
        "lconst_0",
        "lconst_1",
        "fconst_0",
        "fconst_1",
        "fconst_2",
        "dconst_0",
        "dconst_1",
        "bipush",
        "sipush",
        "ldc",
        "ldc_w",
        "ldc2_w",
        "iload",
        "lload",
        "fload",
        "dload",
        "aload",
        "iload_0",
        "iload_1",
        "iload_2",
        "iload_3",
        "lload_0",
        "lload_1",
        "lload_2",
        "lload_3",
        "fload_0",
        "fload_1",
        "fload_2",
        "fload_3",
        "dload_0",
        "dload_1",
        "dload_2",
        "dload_3",
        "aload_0",
        "aload_1",
        "aload_2",
        "aload_3",
        "iaload",
        "laload",
        "faload",
        "daload",
        "aaload",
        "baload",
        "caload",
        "saload",
        "istore",
        "lstore",
        "fstore",
        "dstore",
        "astore",
        "istore_0",
        "istore_1",
        "istore_2",
        "istore_3",
        "lstore_0",
        "lstore_1",
        "lstore_2",
        "lstore_3",
        "fstore_0",
        "fstore_1",
        "fstore_2",
        "fstore_3",
        "dstore_0",
        "dstore_1",
        "dstore_2",
        "dstore_3",
        "astore_0",
        "astore_1",
        "astore_2",
        "astore_3",
        "iastore",
        "lastore",
        "fastore",
        "dastore",
        "aastore",
        "bastore",
        "castore",
        "sastore",
        "pop",
        "pop2",
        "dup",
        "dup_x1",
        "dup_x2",
        "dup2",
        "dup2_x1",
        "dup2_x2",
        "swap",
        "iadd",
        "ladd",
        "fadd",
        "dadd",
        "isub",
        "lsub",
        "fsub",
        "dsub",
        "imul",
        "lmul",
        "fmul",
        "dmul",
        "idiv",
        "ldiv",
        "fdiv",
        "ddiv",
        "irem",
        "lrem",
        "frem",
        "drem",
        "ineg",
        "lneg",
        "fneg",
        "dneg",
        "ishl",
        "lshl",
        "ishr",
        "lshr",
        "iushr",
        "lushr",
        "iand",
        "land",
        "ior",
        "lor",
        "ixor",
        "lxor",
        "iinc",
        "i2l",
        "i2f",
        "i2d",
        "l2i",
        "l2f",
        "l2d",
        "f2i",
        "f2l",
        "f2d",
        "d2i",
        "d2l",
        "d2f",
        "i2b",
        "i2c",
        "i2s",
        "lcmp",
        "fcmpl",
        "fcmpg",
        "dcmpl",
        "dcmpg",
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
        "if_acmpeq",
        "if_acmpne",
        "goto",
        "jsr",
        "ret",
        "tableswitch",
        "lookupswitch",
        "ireturn",
        "lreturn",
        "freturn",
        "dreturn",
        "areturn",
        "return",
        "getstatic",
        "putstatic",
        "getfield",
        "putfield",
        "invokevirtual",
        "invokespecial",
        "invokestatic",
        "invokeinterface",
        "invokedynamic",
        "new",
        "newarray",
        "anewarray",
        "arraylength",
        "athrow",
        "checkcast",
        "instanceof",
        "monitorenter",
        "monitorexit",
        "wide",
        "multianewarray",
        "ifnull",
        "ifnonnull",
        "goto_w",
        "jsr_w",
    ];
    N.get(op as usize).copied()
}

// Class-file CONSTANT_Utf8 uses modified UTF-8 (NUL is C0 80; supplementary
// characters are encoded as UTF-16 surrogate pairs). Decode it explicitly.
fn decode_modified_utf8(bytes: &[u8]) -> Result<String, ClassFileError> {
    let mut units = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let a = bytes[i];
        i += 1;
        let unit = if a & 0x80 == 0 {
            a as u16
        } else if a & 0xe0 == 0xc0 {
            let b = *bytes.get(i).ok_or(ClassFileError::InvalidUtf8)?;
            i += 1;
            if b & 0xc0 != 0x80 {
                return Err(ClassFileError::InvalidUtf8);
            }
            (((a & 0x1f) as u16) << 6) | (b & 0x3f) as u16
        } else if a & 0xf0 == 0xe0 {
            let b = *bytes.get(i).ok_or(ClassFileError::InvalidUtf8)?;
            let c = *bytes.get(i + 1).ok_or(ClassFileError::InvalidUtf8)?;
            i += 2;
            if b & 0xc0 != 0x80 || c & 0xc0 != 0x80 {
                return Err(ClassFileError::InvalidUtf8);
            }
            (((a & 0x0f) as u16) << 12) | (((b & 0x3f) as u16) << 6) | (c & 0x3f) as u16
        } else {
            return Err(ClassFileError::InvalidUtf8);
        };
        units.push(unit);
    }
    String::from_utf16(&units).map_err(|_| ClassFileError::InvalidUtf8)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    #[test]
    fn rejects_bad_magic_and_truncated_files() {
        assert_eq!(
            analyze_class(b"nope").unwrap_err(),
            ClassFileError::BadMagic
        );
        assert_eq!(
            analyze_class(&[0xca, 0xfe, 0xba, 0xbe]).unwrap_err(),
            ClassFileError::Truncated
        );
    }
    #[test]
    fn decodes_modified_utf8_null_and_surrogate_pairs() {
        assert_eq!(decode_modified_utf8(&[0xc0, 0x80]).unwrap(), "\0");
        assert_eq!(
            decode_modified_utf8(&[0xed, 0xa0, 0xbd, 0xed, 0xb8, 0x80]).unwrap(),
            "😀"
        );
    }
    #[test]
    fn opcode_table_covers_defined_instruction_space() {
        assert_eq!(opcode_name(0xb8), Some("invokestatic"));
        assert_eq!(opcode_name(0xca), None);
    }

    #[cfg(test)]
    pub(crate) fn simple_arithmetic_class() -> Vec<u8> {
        // Correct constant-pool layout for a tiny test class containing:
        //   public static int add(int, int)  -> pure integer arithmetic (supported)
        //   public static void calls()       -> invokestatic Math.abs (unsupported)
        vec![
            0xca, 0xfe, 0xba, 0xbe, 0x00, 0x00, 0x00, 0x3d, 0x00, 0x13, 0x01, 0x00, 0x07, 0x45,
            0x78, 0x61, 0x6d, 0x70, 0x6c, 0x65, 0x07, 0x00, 0x01, 0x01, 0x00, 0x10, 0x6a, 0x61,
            0x76, 0x61, 0x2f, 0x6c, 0x61, 0x6e, 0x67, 0x2f, 0x4f, 0x62, 0x6a, 0x65, 0x63, 0x74,
            0x07, 0x00, 0x03, 0x01, 0x00, 0x06, 0x3c, 0x69, 0x6e, 0x69, 0x74, 0x3e, 0x01, 0x00,
            0x03, 0x28, 0x29, 0x56, 0x0c, 0x00, 0x05, 0x00, 0x06, 0x0a, 0x00, 0x04, 0x00, 0x07,
            0x01, 0x00, 0x04, 0x43, 0x6f, 0x64, 0x65, 0x01, 0x00, 0x03, 0x61, 0x64, 0x64, 0x01,
            0x00, 0x05, 0x28, 0x49, 0x49, 0x29, 0x49, 0x01, 0x00, 0x05, 0x63, 0x61, 0x6c, 0x6c,
            0x73, 0x01, 0x00, 0x0e, 0x6a, 0x61, 0x76, 0x61, 0x2f, 0x6c, 0x61, 0x6e, 0x67, 0x2f,
            0x4d, 0x61, 0x74, 0x68, 0x07, 0x00, 0x0d, 0x01, 0x00, 0x03, 0x61, 0x62, 0x73, 0x01,
            0x00, 0x04, 0x28, 0x49, 0x29, 0x49, 0x0c, 0x00, 0x0f, 0x00, 0x10, 0x0a, 0x00, 0x0e,
            0x00, 0x11, 0x00, 0x21, 0x00, 0x02, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03,
            0x00, 0x01, 0x00, 0x05, 0x00, 0x06, 0x00, 0x01, 0x00, 0x09, 0x00, 0x00, 0x00, 0x11,
            0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x05, 0x2a, 0xb7, 0x00, 0x08, 0xb1, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x09, 0x00, 0x0a, 0x00, 0x0b, 0x00, 0x01, 0x00, 0x09, 0x00,
            0x00, 0x00, 0x10, 0x00, 0x02, 0x00, 0x02, 0x00, 0x00, 0x00, 0x04, 0x1a, 0x1b, 0x60,
            0xac, 0x00, 0x00, 0x00, 0x00, 0x00, 0x09, 0x00, 0x0c, 0x00, 0x06, 0x00, 0x01, 0x00,
            0x09, 0x00, 0x00, 0x00, 0x12, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x06, 0x04,
            0xb8, 0x00, 0x12, 0x57, 0xb1, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ]
    }

    #[test]
    fn inventories_static_and_instance_field_access_references() {
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

        // Example.read() reads sample/State.count via getstatic.
        let mut b = Vec::new();
        u4(&mut b, 0xcafebabe);
        u2(&mut b, 0);
        u2(&mut b, 52);
        u2(&mut b, 14);
        utf8(&mut b, "Example"); // #1
        b.push(7);
        u2(&mut b, 1); // #2 Class Example
        utf8(&mut b, "java/lang/Object"); // #3
        b.push(7);
        u2(&mut b, 3); // #4 Class Object
        utf8(&mut b, "Code"); // #5
        utf8(&mut b, "read"); // #6
        utf8(&mut b, "()I"); // #7
        utf8(&mut b, "sample/State"); // #8
        b.push(7);
        u2(&mut b, 8); // #9 Class State
        utf8(&mut b, "count"); // #10
        utf8(&mut b, "I"); // #11
        b.push(12);
        u2(&mut b, 10);
        u2(&mut b, 11); // #12 NameAndType
        b.push(9);
        u2(&mut b, 9);
        u2(&mut b, 12); // #13 Fieldref

        u2(&mut b, 0x0021); // public super
        u2(&mut b, 2); // this
        u2(&mut b, 4); // super
        u2(&mut b, 0); // interfaces
        u2(&mut b, 0); // fields
        u2(&mut b, 1); // methods
        u2(&mut b, 0x0009); // public static
        u2(&mut b, 6);
        u2(&mut b, 7);
        u2(&mut b, 1); // attributes
        u2(&mut b, 5); // Code
        u4(&mut b, 16); // Code attribute length
        u2(&mut b, 1); // max_stack
        u2(&mut b, 0); // max_locals
        u4(&mut b, 4); // code length
        b.extend([0xb2, 0x00, 0x0d, 0xac]); // getstatic #13; ireturn
        u2(&mut b, 0); // exception table
        u2(&mut b, 0); // nested attributes
        u2(&mut b, 0); // class attributes

        let class = analyze_class(&b).unwrap();
        assert_eq!(
            class.field_references,
            BTreeSet::from(["sample/State.count:I [getstatic]".to_owned()])
        );
        assert_eq!(class.methods[0].field_accesses.len(), 1);
        assert_eq!(class.methods[0].field_accesses[0].owner, "sample/State");
        assert_eq!(class.methods[0].field_accesses[0].name, "count");
        assert_eq!(class.methods[0].field_accesses[0].opcode, "getstatic");

        let jar = analyze_jar_classes([("Example.class", b)]);
        assert!(
            jar.field_references
                .contains("sample/State.count:I [getstatic]")
        );
    }

    #[test]
    fn parses_class_header_methods_and_invocation_references() {
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
        let mut b = Vec::new();
        u4(&mut b, 0xcafebabe);
        u2(&mut b, 0);
        u2(&mut b, 61);
        u2(&mut b, 12);
        utf8(&mut b, "Example");
        b.push(7);
        u2(&mut b, 1);
        utf8(&mut b, "java/lang/Object");
        b.push(7);
        u2(&mut b, 3);
        utf8(&mut b, "<init>");
        utf8(&mut b, "()V");
        b.push(12);
        u2(&mut b, 5);
        u2(&mut b, 6);
        b.push(10);
        u2(&mut b, 4);
        u2(&mut b, 7);
        utf8(&mut b, "Code");
        utf8(&mut b, "run");
        utf8(&mut b, "()V");
        u2(&mut b, 0x0021);
        u2(&mut b, 2);
        u2(&mut b, 4);
        u2(&mut b, 0);
        u2(&mut b, 0);
        // public <init>()V: aload_0; invokespecial Object.<init>; return
        u2(&mut b, 2);
        u2(&mut b, 0x0001);
        u2(&mut b, 5);
        u2(&mut b, 6);
        u2(&mut b, 1);
        u2(&mut b, 9);
        u4(&mut b, 17);
        u2(&mut b, 1);
        u2(&mut b, 1);
        u4(&mut b, 5);
        b.extend([0x2a, 0xb7, 0x00, 0x08, 0xb1]);
        u2(&mut b, 0);
        u2(&mut b, 0);
        // public static run()V: iconst_0; pop; return
        u2(&mut b, 0x0009);
        u2(&mut b, 10);
        u2(&mut b, 11);
        u2(&mut b, 1);
        u2(&mut b, 9);
        u4(&mut b, 15);
        u2(&mut b, 1);
        u2(&mut b, 1);
        u4(&mut b, 3);
        b.extend([0x03, 0x57, 0xb1]);
        u2(&mut b, 0);
        u2(&mut b, 0);
        u2(&mut b, 0);

        let class = analyze_class(&b).unwrap();
        assert_eq!(class.name, "Example");
        assert_eq!(class.super_name.as_deref(), Some("java/lang/Object"));
        assert_eq!(class.methods.len(), 2);
        assert_eq!(class.methods[0].instruction_count, 3);
        assert_eq!(class.methods[0].invoked_methods[0].name, "<init>");
        assert!(class.opcode_counts.contains_key("invokespecial"));
    }
}
