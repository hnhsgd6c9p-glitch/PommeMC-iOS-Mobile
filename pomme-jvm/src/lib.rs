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
                0x1a..=0x1d => {\n                    let index = (op - 0x1a) as usize;\n                    check_local(index, self.limits.max_locals)?;\n                    push(&mut stack, locals[index], self.limits.max_stack)?;\n                }
                0x3b..=0x3e => {\n                    let index = (op - 0x3b) as usize;\n                    check_local(index, self.limits.max_locals)?;\n                    locals[index] = pop(&mut stack)?;\n                }
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

#[cfg(test)]
mod tests {
    use super::*;

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
