//! Typed, bounded intermediate representation for the supported integer JVM
//! subset.
//!
//! This is a translation boundary, not a JVM. Branches retain bytecode offsets
//! so a later Rust code generator can construct basic blocks without
//! interpreting arbitrary Java object semantics.

use std::collections::BTreeSet;

use thiserror::Error;

use crate::classfile::{ClassFileError, MethodSummary, analyze_class};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum IrError {
    #[error("class file could not be analyzed: {0}")]
    ClassFile(#[from] ClassFileError),
    #[error("requested method was not found")]
    MethodNotFound,
    #[error("method signature is outside the integer subset")]
    UnsupportedSignature,
    #[error("method is not static")]
    NotStatic,
    #[error("method has no bytecode body")]
    NoCode,
    #[error("unsupported bytecode instruction {0:#04x}")]
    UnsupportedOpcode(u8),
    #[error("malformed instruction or branch target")]
    InvalidBytecode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnaryCondition {
    EqualZero,
    NotEqualZero,
    LessThanZero,
    GreaterOrEqualZero,
    GreaterThanZero,
    LessOrEqualZero,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinaryCondition {
    Equal,
    NotEqual,
    LessThan,
    GreaterOrEqual,
    GreaterThan,
    LessOrEqual,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IrInstruction {
    ConstI32(i32),
    LoadLocal(u8),
    StoreLocal(u8),
    Pop,
    Dup,
    AddI32,
    SubI32,
    MulI32,
    DivI32,
    RemI32,
    NegI32,
    ShiftLeftI32,
    ShiftRightI32,
    UnsignedShiftRightI32,
    AndI32,
    OrI32,
    XorI32,
    IncrementLocal {
        index: u8,
        amount: i8,
    },
    BranchIfZero {
        condition: UnaryCondition,
        target_pc: usize,
    },
    BranchIfCompare {
        condition: BinaryCondition,
        target_pc: usize,
    },
    Goto {
        target_pc: usize,
    },
    ReturnI32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocatedInstruction {
    pub bytecode_pc: usize,
    pub instruction: IrInstruction,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MethodIr {
    pub class_name: String,
    pub method_name: String,
    pub descriptor: String,
    pub max_stack: u16,
    pub max_locals: u16,
    pub instructions: Vec<LocatedInstruction>,
}

/// Lower one static (int, ... int) -> int Java method into typed IR.
/// Every branch target is checked to land on an instruction boundary.
pub fn lower_static_i32_method(
    class_bytes: &[u8],
    method_name: &str,
    descriptor: &str,
) -> Result<MethodIr, IrError> {
    let class = analyze_class(class_bytes)?;
    let method = class
        .methods
        .iter()
        .find(|m| m.name == method_name && m.descriptor == descriptor)
        .ok_or(IrError::MethodNotFound)?;
    lower_method(&class.name, method)
}

fn lower_method(class_name: &str, method: &MethodSummary) -> Result<MethodIr, IrError> {
    if method.access_flags & 0x0008 == 0 {
        return Err(IrError::NotStatic);
    }
    let (params, result) = method
        .descriptor
        .split_once(')')
        .ok_or(IrError::UnsupportedSignature)?;
    if !params.starts_with('(') || !params[1..].chars().all(|c| c == 'I') || result != "I" {
        return Err(IrError::UnsupportedSignature);
    }
    let parameter_count = params.len().saturating_sub(1);
    if parameter_count > method.max_locals as usize {
        return Err(IrError::InvalidBytecode);
    }
    if method.code.is_empty() {
        return Err(IrError::NoCode);
    }

    let code = &method.code;
    let mut pc = 0usize;
    let mut instructions = Vec::new();
    let mut instruction_pcs = BTreeSet::new();
    let mut branch_targets = Vec::new();
    while pc < code.len() {
        let start = pc;
        instruction_pcs.insert(start);
        let op = code[pc];
        pc += 1;
        let instruction = match op {
            0x02..=0x08 => IrInstruction::ConstI32(op as i32 - 3),
            0x10 => {
                let value = *code.get(pc).ok_or(IrError::InvalidBytecode)? as i8 as i32;
                pc += 1;
                IrInstruction::ConstI32(value)
            }
            0x11 => {
                let value = read_i16(code, pc)? as i32;
                pc += 2;
                IrInstruction::ConstI32(value)
            }
            0x15 => IrInstruction::LoadLocal(read_u8(code, &mut pc)?),
            0x1a..=0x1d => IrInstruction::LoadLocal(op - 0x1a),
            0x36 => IrInstruction::StoreLocal(read_u8(code, &mut pc)?),
            0x3b..=0x3e => IrInstruction::StoreLocal(op - 0x3b),
            0x57 => IrInstruction::Pop,
            0x59 => IrInstruction::Dup,
            0x60 => IrInstruction::AddI32,
            0x64 => IrInstruction::SubI32,
            0x68 => IrInstruction::MulI32,
            0x6c => IrInstruction::DivI32,
            0x70 => IrInstruction::RemI32,
            0x74 => IrInstruction::NegI32,
            0x78 => IrInstruction::ShiftLeftI32,
            0x7a => IrInstruction::ShiftRightI32,
            0x7c => IrInstruction::UnsignedShiftRightI32,
            0x7e => IrInstruction::AndI32,
            0x80 => IrInstruction::OrI32,
            0x82 => IrInstruction::XorI32,
            0x84 => {
                let index = read_u8(code, &mut pc)?;
                let amount = read_u8(code, &mut pc)? as i8;
                IrInstruction::IncrementLocal { index, amount }
            }
            0x99..=0x9e => {
                let offset = read_i16(code, pc)?;
                pc += 2;
                let target_pc = branch_target(start, offset, code.len())?;
                branch_targets.push(target_pc);
                let condition = match op {
                    0x99 => UnaryCondition::EqualZero,
                    0x9a => UnaryCondition::NotEqualZero,
                    0x9b => UnaryCondition::LessThanZero,
                    0x9c => UnaryCondition::GreaterOrEqualZero,
                    0x9d => UnaryCondition::GreaterThanZero,
                    _ => UnaryCondition::LessOrEqualZero,
                };
                IrInstruction::BranchIfZero {
                    condition,
                    target_pc,
                }
            }
            0x9f..=0xa4 => {
                let offset = read_i16(code, pc)?;
                pc += 2;
                let target_pc = branch_target(start, offset, code.len())?;
                branch_targets.push(target_pc);
                let condition = match op {
                    0x9f => BinaryCondition::Equal,
                    0xa0 => BinaryCondition::NotEqual,
                    0xa1 => BinaryCondition::LessThan,
                    0xa2 => BinaryCondition::GreaterOrEqual,
                    0xa3 => BinaryCondition::GreaterThan,
                    _ => BinaryCondition::LessOrEqual,
                };
                IrInstruction::BranchIfCompare {
                    condition,
                    target_pc,
                }
            }
            0xa7 => {
                let offset = read_i16(code, pc)?;
                pc += 2;
                let target_pc = branch_target(start, offset, code.len())?;
                branch_targets.push(target_pc);
                IrInstruction::Goto { target_pc }
            }
            0xac => IrInstruction::ReturnI32,
            other => return Err(IrError::UnsupportedOpcode(other)),
        };
        if pc > code.len() || pc <= start {
            return Err(IrError::InvalidBytecode);
        }
        let local_index = match &instruction {
            IrInstruction::LoadLocal(index) | IrInstruction::StoreLocal(index) => Some(*index),
            IrInstruction::IncrementLocal { index, .. } => Some(*index),
            _ => None,
        };
        if local_index.is_some_and(|index| index as u16 >= method.max_locals) {
            return Err(IrError::InvalidBytecode);
        }
        instructions.push(LocatedInstruction {
            bytecode_pc: start,
            instruction,
        });
    }

    for target in branch_targets {
        if !instruction_pcs.contains(&target) {
            return Err(IrError::InvalidBytecode);
        }
    }

    Ok(MethodIr {
        class_name: class_name.to_owned(),
        method_name: method.name.clone(),
        descriptor: method.descriptor.clone(),
        max_stack: method.max_stack,
        max_locals: method.max_locals,
        instructions,
    })
}

fn read_u8(code: &[u8], pc: &mut usize) -> Result<u8, IrError> {
    let value = *code.get(*pc).ok_or(IrError::InvalidBytecode)?;
    *pc += 1;
    Ok(value)
}

fn read_i16(code: &[u8], pc: usize) -> Result<i16, IrError> {
    let bytes = code.get(pc..pc + 2).ok_or(IrError::InvalidBytecode)?;
    Ok(i16::from_be_bytes(bytes.try_into().unwrap()))
}

fn branch_target(origin: usize, offset: i16, code_len: usize) -> Result<usize, IrError> {
    let target = (origin as i64)
        .checked_add(offset as i64)
        .ok_or(IrError::InvalidBytecode)?;
    if target < 0 || target as usize >= code_len {
        return Err(IrError::InvalidBytecode);
    }
    Ok(target as usize)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classfile::tests::simple_arithmetic_class;

    #[test]
    fn lowers_static_add_method_into_typed_ir() {
        let ir = lower_static_i32_method(&simple_arithmetic_class(), "add", "(II)I").unwrap();
        assert_eq!(ir.class_name, "Example");
        assert_eq!(
            ir.instructions
                .iter()
                .map(|i| i.instruction.clone())
                .collect::<Vec<_>>(),
            vec![
                IrInstruction::LoadLocal(0),
                IrInstruction::LoadLocal(1),
                IrInstruction::AddI32,
                IrInstruction::ReturnI32,
            ]
        );
    }

    #[test]
    fn lowers_conditional_branches_and_rejects_mid_instruction_targets() {
        let method = MethodSummary {
            name: "absolute".into(),
            descriptor: "(I)I".into(),
            access_flags: 0x0009,
            bytecode_bytes: 9,
            instruction_count: 6,
            max_stack: 1,
            max_locals: 1,
            code: vec![0x1a, 0x9c, 0x00, 0x06, 0x1a, 0x74, 0xac, 0x1a, 0xac],
            invoked_methods: Vec::new(),
            field_accesses: Vec::new(),
        };
        let ir = lower_method("Example", &method).unwrap();
        assert!(ir.instructions.iter().any(|located| {
            located.instruction
                == IrInstruction::BranchIfZero {
                    condition: UnaryCondition::GreaterOrEqualZero,
                    target_pc: 7,
                }
        }));

        let malformed = MethodSummary {
            code: vec![0x1a, 0x9c, 0x00, 0x01, 0x1a, 0xac],
            bytecode_bytes: 6,
            ..method
        };
        assert_eq!(
            lower_method("Example", &malformed),
            Err(IrError::InvalidBytecode)
        );
    }

    #[test]
    fn rejects_local_indexes_outside_declared_frame() {
        let method = MethodSummary {
            name: "badLocal".into(),
            descriptor: "()I".into(),
            access_flags: 0x0009,
            bytecode_bytes: 2,
            instruction_count: 2,
            max_stack: 1,
            max_locals: 0,
            code: vec![0x1a, 0xac],
            invoked_methods: Vec::new(),
            field_accesses: Vec::new(),
        };
        assert_eq!(
            lower_method("Example", &method),
            Err(IrError::InvalidBytecode)
        );
    }

    #[test]
    fn rejects_non_integer_methods_before_lowering() {
        assert_eq!(
            lower_static_i32_method(&simple_arithmetic_class(), "calls", "()V"),
            Err(IrError::UnsupportedSignature)
        );
    }
}
