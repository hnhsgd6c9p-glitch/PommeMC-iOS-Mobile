//! Small, bounded interpreter for a deliberately narrow Java bytecode subset.
//!
//! This is a bridge for pure static integer helpers, not a JVM. It has no heap,
//! object model, reflection, class loading, threads, native calls, or I/O.

use thiserror::Error;

use crate::classfile::{ClassFileError, analyze_class};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum JavaSubsetError {
    #[error("class file could not be analyzed: {0}")]
    ClassFile(#[from] ClassFileError),
    #[error("requested method was not found")]
    MethodNotFound,
    #[error("only static methods with integer parameters and an integer return are supported")]
    UnsupportedSignature,
    #[error("method is not static")]
    NotStatic,
    #[error("argument count does not match the method descriptor")]
    ArgumentCount,
    #[error("method has no bytecode body")]
    NoCode,
    #[error("unsupported Java bytecode instruction {0:#04x}")]
    UnsupportedOpcode(u8),
    #[error("invalid bytecode or branch target")]
    InvalidBytecode,
    #[error("execution exceeded its instruction budget")]
    FuelExhausted,
    #[error("operand stack or local variable bounds exceeded")]
    StackOrLocalBounds,
    #[error("integer division by zero")]
    DivisionByZero,
}

/// Execute a static Java method with `int` parameters and an `int` result.
/// Every executed JVM instruction consumes one unit of fuel. Only a bounded
/// arithmetic/control-flow subset is accepted; calls, heap access, and I/O are
/// rejected. Integer overflow follows JVM wrapping semantics.
pub fn execute_static_i32(
    class_bytes: &[u8],
    method_name: &str,
    descriptor: &str,
    args: &[i32],
    fuel: u64,
) -> Result<i32, JavaSubsetError> {
    execute_static_i32_with_fuel(class_bytes, method_name, descriptor, args, fuel)
        .map(|(value, _)| value)
}

/// Execute a static integer helper and report the amount of fuel consumed.
pub fn execute_static_i32_with_fuel(
    class_bytes: &[u8],
    method_name: &str,
    descriptor: &str,
    args: &[i32],
    fuel: u64,
) -> Result<(i32, u64), JavaSubsetError> {
    let class = analyze_class(class_bytes)?;
    let method = class
        .methods
        .iter()
        .find(|method| method.name == method_name && method.descriptor == descriptor)
        .ok_or(JavaSubsetError::MethodNotFound)?;
    if method.access_flags & 0x0008 == 0 {
        return Err(JavaSubsetError::NotStatic);
    }
    validate_int_descriptor(descriptor)?;
    let expected_args = descriptor
        .split(')')
        .next()
        .ok_or(JavaSubsetError::UnsupportedSignature)?
        .matches('I')
        .count();
    if args.len() != expected_args {
        return Err(JavaSubsetError::ArgumentCount);
    }
    if method.code.is_empty() {
        return Err(JavaSubsetError::NoCode);
    }

    let mut locals = vec![0i32; method.max_locals as usize];
    if args.len() > locals.len() {
        return Err(JavaSubsetError::StackOrLocalBounds);
    }
    locals[..args.len()].copy_from_slice(args);
    let mut stack = Vec::<i32>::with_capacity(method.max_stack as usize);
    let mut pc = 0usize;
    let initial_fuel = fuel;
    let mut remaining = fuel;
    let code = &method.code;

    loop {
        if remaining == 0 {
            return Err(JavaSubsetError::FuelExhausted);
        }
        remaining -= 1;
        let op_pc = pc;
        let op = *code.get(pc).ok_or(JavaSubsetError::InvalidBytecode)?;
        pc += 1;
        match op {
            0x02..=0x08 => push(&mut stack, (op as i32) - 3, method.max_stack)?,
            0x10 => {
                let value = *code.get(pc).ok_or(JavaSubsetError::InvalidBytecode)? as i8 as i32;
                pc += 1;
                push(&mut stack, value, method.max_stack)?;
            }
            0x11 => {
                let value = read_i16(code, pc)? as i32;
                pc += 2;
                push(&mut stack, value, method.max_stack)?;
            }
            0x15 => {
                let index = read_local_index(code, &mut pc)?;
                load(&locals, index, &mut stack, method.max_stack)?;
            }
            0x1a..=0x1d => load(&locals, (op - 0x1a) as usize, &mut stack, method.max_stack)?,
            0x36 => {
                let index = read_local_index(code, &mut pc)?;
                store(&mut locals, index, &mut stack)?;
            }
            0x3b..=0x3e => store(&mut locals, (op - 0x3b) as usize, &mut stack)?,
            0x57 => {
                pop(&mut stack)?;
            }
            0x59 => {
                let value = *stack.last().ok_or(JavaSubsetError::StackOrLocalBounds)?;
                push(&mut stack, value, method.max_stack)?;
            }
            0x60 => binary(&mut stack, i32::wrapping_add)?,
            0x64 => binary(&mut stack, i32::wrapping_sub)?,
            0x68 => binary(&mut stack, i32::wrapping_mul)?,
            0x6c => {
                let rhs = pop(&mut stack)?;
                let lhs = pop(&mut stack)?;
                if rhs == 0 {
                    return Err(JavaSubsetError::DivisionByZero);
                }
                push(&mut stack, lhs.wrapping_div(rhs), method.max_stack)?;
            }
            0x70 => {
                let rhs = pop(&mut stack)?;
                let lhs = pop(&mut stack)?;
                if rhs == 0 {
                    return Err(JavaSubsetError::DivisionByZero);
                }
                push(&mut stack, lhs.wrapping_rem(rhs), method.max_stack)?;
            }
            0x74 => {
                let value = pop(&mut stack)?;
                push(&mut stack, value.wrapping_neg(), method.max_stack)?;
            }
            0x78 => binary(&mut stack, |a, b| a.wrapping_shl((b as u32) & 31))?,
            0x7a => binary(&mut stack, |a, b| a.wrapping_shr((b as u32) & 31))?,
            0x7c => binary(&mut stack, |a, b| {
                ((a as u32).wrapping_shr((b as u32) & 31)) as i32
            })?,
            0x7e => binary(&mut stack, |a, b| a & b)?,
            0x80 => binary(&mut stack, |a, b| a | b)?,
            0x82 => binary(&mut stack, |a, b| a ^ b)?,
            0x84 => {
                let index = read_local_index(code, &mut pc)?;
                let amount = *code.get(pc).ok_or(JavaSubsetError::InvalidBytecode)? as i8 as i32;
                pc += 1;
                let value = locals
                    .get_mut(index)
                    .ok_or(JavaSubsetError::StackOrLocalBounds)?;
                *value = value.wrapping_add(amount);
            }
            0x99..=0x9e => {
                let offset = read_i16(code, pc)?;
                pc += 2;
                let value = pop(&mut stack)?;
                let take = match op {
                    0x99 => value == 0,
                    0x9a => value != 0,
                    0x9b => value < 0,
                    0x9c => value >= 0,
                    0x9d => value > 0,
                    _ => value <= 0,
                };
                if take {
                    pc = branch_target(op_pc, offset, code.len())?;
                }
            }
            0x9f..=0xa4 => {
                let offset = read_i16(code, pc)?;
                pc += 2;
                let rhs = pop(&mut stack)?;
                let lhs = pop(&mut stack)?;
                let take = match op {
                    0x9f => lhs == rhs,
                    0xa0 => lhs != rhs,
                    0xa1 => lhs < rhs,
                    0xa2 => lhs >= rhs,
                    0xa3 => lhs > rhs,
                    _ => lhs <= rhs,
                };
                if take {
                    pc = branch_target(op_pc, offset, code.len())?;
                }
            }
            0xa7 => {
                let offset = read_i16(code, pc)?;
                pc = branch_target(op_pc, offset, code.len())?;
            }
            0xac => return Ok((pop(&mut stack)?, initial_fuel - remaining)),
            other => return Err(JavaSubsetError::UnsupportedOpcode(other)),
        }
        if pc > code.len() {
            return Err(JavaSubsetError::InvalidBytecode);
        }
    }
}

fn validate_int_descriptor(descriptor: &str) -> Result<(), JavaSubsetError> {
    let (params, result) = descriptor
        .split_once(')')
        .ok_or(JavaSubsetError::UnsupportedSignature)?;
    if !params.starts_with('(') || !params[1..].chars().all(|c| c == 'I') || result != "I" {
        return Err(JavaSubsetError::UnsupportedSignature);
    }
    Ok(())
}
fn push(stack: &mut Vec<i32>, value: i32, max: u16) -> Result<(), JavaSubsetError> {
    if stack.len() >= max as usize {
        return Err(JavaSubsetError::StackOrLocalBounds);
    }
    stack.push(value);
    Ok(())
}
fn pop(stack: &mut Vec<i32>) -> Result<i32, JavaSubsetError> {
    stack.pop().ok_or(JavaSubsetError::StackOrLocalBounds)
}
fn load(
    locals: &[i32],
    index: usize,
    stack: &mut Vec<i32>,
    max: u16,
) -> Result<(), JavaSubsetError> {
    let value = *locals
        .get(index)
        .ok_or(JavaSubsetError::StackOrLocalBounds)?;
    push(stack, value, max)
}
fn store(locals: &mut [i32], index: usize, stack: &mut Vec<i32>) -> Result<(), JavaSubsetError> {
    let value = pop(stack)?;
    *locals
        .get_mut(index)
        .ok_or(JavaSubsetError::StackOrLocalBounds)? = value;
    Ok(())
}
fn binary(stack: &mut Vec<i32>, f: impl FnOnce(i32, i32) -> i32) -> Result<(), JavaSubsetError> {
    let rhs = pop(stack)?;
    let lhs = pop(stack)?;
    stack.push(f(lhs, rhs));
    Ok(())
}
fn read_local_index(code: &[u8], pc: &mut usize) -> Result<usize, JavaSubsetError> {
    let value = *code.get(*pc).ok_or(JavaSubsetError::InvalidBytecode)? as usize;
    *pc += 1;
    Ok(value)
}
fn read_i16(code: &[u8], pc: usize) -> Result<i16, JavaSubsetError> {
    Ok(i16::from_be_bytes(
        code.get(pc..pc + 2)
            .ok_or(JavaSubsetError::InvalidBytecode)?
            .try_into()
            .unwrap(),
    ))
}
fn branch_target(origin: usize, offset: i16, len: usize) -> Result<usize, JavaSubsetError> {
    let target = (origin as i64)
        .checked_add(offset as i64)
        .ok_or(JavaSubsetError::InvalidBytecode)?;
    if target < 0 || target as usize >= len {
        return Err(JavaSubsetError::InvalidBytecode);
    }
    Ok(target as usize)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classfile::tests::simple_arithmetic_class;

    #[test]
    fn executes_a_compiled_style_static_integer_method() {
        let bytes = simple_arithmetic_class();
        assert_eq!(
            execute_static_i32(&bytes, "add", "(II)I", &[19, 23], 20).unwrap(),
            42
        );
    }

    #[test]
    fn enforces_instruction_fuel() {
        let bytes = simple_arithmetic_class();
        assert_eq!(
            execute_static_i32(&bytes, "add", "(II)I", &[1, 2], 1),
            Err(JavaSubsetError::FuelExhausted)
        );
    }

    #[test]
    fn rejects_calls_outside_the_supported_subset() {
        let bytes = simple_arithmetic_class();
        // Method exists but contains invokestatic, which is outside the pure integer
        // subset.
        assert_eq!(
            execute_static_i32(&bytes, "calls", "()V", &[], 20),
            Err(JavaSubsetError::UnsupportedSignature)
        );
    }
}
