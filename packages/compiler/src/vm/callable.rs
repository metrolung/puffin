use std::cell::UnsafeCell;
use std::collections::HashMap;
use std::fmt::{Debug, Formatter, Write};
use crate::common::raw_value::{StrongValue, Value, ValueArray};
use crate::common::value::{CallableObjectHeader, ObjectFlag, StringObjectHeader};
use crate::compiler::codegen::Instruction;
use crate::vm::garbage_collector::{Heap, Stack};

pub struct Runtime<'heap, 'obj> {
    pub program: &'obj PuffinProgram,
    pub heap: &'heap mut Heap<'obj>,
    stack: Stack<'obj>,
}


pub trait PuffinCallable {
    fn invoke(&self, runtime: &mut Runtime, frame_start: usize) -> Result<(), RuntimeError>;
    fn debug(&self) -> Option<&dyn Debug> { None }
}

#[derive(Debug)]
pub struct RuntimeError(pub String);

pub struct CallContext<'runtime, 'heap, 'obj> {
    pub runtime: &'runtime mut Runtime<'heap, 'obj>,
    frame_start: usize
}

impl<'runtime, 'heap, 'obj> CallContext<'runtime, 'heap, 'obj> {
    pub fn invoke(&mut self, value: StrongValue, params: Vec<StrongValue<'obj>>) -> Result<Vec<StrongValue<'obj>>, RuntimeError> {
        for param in params {
            self.runtime.stack.push(param.value())?;
        }

        let param0 = self.runtime.stack.get_ptr();
        value.value().invoke(self.runtime, self.frame_start)?;
        let param_n = self.runtime.stack.get_ptr();

        Ok(self.runtime.stack.pop_many(param_n - param0)?.to_strong_vec())
    }
}

pub enum FunctionHelper {}

impl FunctionHelper {
    pub fn new(callable: impl for <'obj> Fn(CallContext<'_, '_, 'obj>, Vec<StrongValue<'obj>>) -> Result<Vec<StrongValue<'obj>>, RuntimeError> + 'static) -> CallableObjectHeader {
        CallableObjectHeader::new(
            Box::new(callable) as Box<dyn PuffinCallable>
        )
    }
}

impl<T> PuffinCallable for T
where T: for <'obj> Fn(CallContext<'_, '_, 'obj>, Vec<StrongValue<'obj>>) -> (Result<Vec<StrongValue<'obj>>, RuntimeError>) {
    fn invoke(&self, runtime: &mut Runtime, frame_start: usize) -> Result<(), RuntimeError> {
        let values = runtime.stack.pop_many(runtime.stack.get_ptr()-frame_start)?;
        
        let invoker = CallContext {
            runtime,
            frame_start
        };

        let result = self(invoker, values.to_strong_vec())?;

        for i in 0..result.len() {
            runtime.stack.push(result[i].value())?;
        }

        Ok(())
    }
}

impl PuffinCallable for Value<'_> {
    fn invoke(&self, runtime: &mut Runtime, frame_start: usize) -> Result<(), RuntimeError> {
        let Some(obj) = self.reference() else {
            return Err(RuntimeError("cannot dereference non ptr".to_string()))
        };

        unsafe { (*obj.callable).invoke(runtime, frame_start) }
    }

    fn debug(&self) -> Option<&dyn Debug> {
        Some(self as &dyn Debug)
    }
}

impl PuffinCallable for CallableObjectHeader {
    fn invoke(&self, runtime: &mut Runtime, frame_start: usize) -> Result<(), RuntimeError> {
        PuffinCallable::invoke(&*self.value, runtime, frame_start)
    }

    fn debug(&self) -> Option<&dyn Debug> {
        self.value.debug()
    }
}

#[derive(Debug)]
pub struct PuffinProgram {
    pub string_table: Vec<StringObjectHeader>,
    pub function_table: Vec<CallableObjectHeader>,
    pub function_lookup: HashMap<String, usize>,
    // pub entry_function: usize,
    // pub static_pool: StaticPool
}

impl PuffinProgram {
    pub fn link_function(&mut self, name: String, callable: CallableObjectHeader) -> Result<(), RuntimeError> {
        let function_idx = self.function_lookup.get(&name).ok_or(RuntimeError(format!("could not find function {}", name)))?;
        self.function_table[*function_idx] = callable;

        Ok(())
    }

    pub fn execute<'obj>(&'obj self, heap: &mut Heap<'obj>, name: &str) -> Result<ValueArray<'obj>, RuntimeError> {
        let mut puffin_thread = Runtime {
            program: &self,
            heap,
            stack: Stack::new(),
        };

        let function_idx = self.function_lookup.get(name).ok_or(RuntimeError(format!("could not find function {}", name)))?;
        let function = &self.function_table[*function_idx];
        function.value.invoke(&mut puffin_thread, 0)?;

        // let mut returns = vec![Value::Void; puffin_thread.stack.get_ptr()];
        let values = puffin_thread.stack.get_many(0..puffin_thread.stack.get_ptr())?;

        Ok(values)
    }
}

// impl<'pool> PuffinCallable for PuffinProgram<'pool> {
//     fn invoke(&self, runtime: &mut Runtime, static_pool: &GCPool, frame_start: usize) -> Result<(), RuntimeError> {
//         let entry = self.entry_function;
//         static_pool.get_object(entry).invoke(runtime, static_pool, frame_start)?;
//         Ok(())
//     }
// }

#[derive(Debug)]
pub struct PuffinFunction {
    pub instructions: Vec<Instruction>
}

impl PuffinCallable for PuffinFunction {
    fn invoke(&self, runtime: &mut Runtime, frame_start: usize) -> Result<(), RuntimeError> {
        let mut instruction_idx = 0usize;

        loop {
            let instruction = self.instructions.get(instruction_idx).ok_or(RuntimeError("instruction out of bounds".to_string()))?;

            {
                let stack = runtime.stack.data.get_subset(0..runtime.stack.get_ptr()).to_vec();

                drop(stack);
                // println!("{:?}", stack);
            }

            instruction_idx += 1;

            match instruction {
                Instruction::Load(i) => {
                    runtime.stack.push((*i).into())?
                }
                Instruction::LoadString(idx) => {
                    runtime.stack.push((&runtime.program.string_table[*idx]).into())?
                }
                Instruction::LoadObject(idx) => {
                    todo!()
                    // runtime.stack.push(runtime.static_pool.pool.get_object(*idx))?
                }
                Instruction::LoadFunction(idx) => {
                    let value = (&runtime.program.function_table[*idx]).into();
                    runtime.stack.push(value)?
                }
                Instruction::LoadLocal(local_idx, size) => {
                    let adr = frame_start + *local_idx as usize;
                    let values = runtime.stack.get_many(adr..adr+*size as usize)?;
                    for i in 0..values.len() {
                        runtime.stack.push(values.get(i))?;
                    }
                }
                Instruction::Invoke(callable_idx, param_size) => {
                    let new_frame_start = runtime.stack.get_ptr() - *param_size as usize;
                    let callable = runtime.stack.get(frame_start + *callable_idx as usize)?;

                    callable.invoke(runtime, new_frame_start)?;
                }
                Instruction::Return(return_size) => {
                    runtime.stack.move_back_ptr(frame_start + (*return_size as usize))?;
                    return Ok(());
                }
                Instruction::Reposition(idx) => {
                    runtime.stack.move_back_ptr(frame_start + *idx as usize)?;
                }
                Instruction::CopyLocal(src, dest, size) => {
                    runtime.stack.copy(
                        frame_start + *src as usize,
                        frame_start + *dest as usize,
                        *size as usize
                    )?;
                }
                Instruction::CopyLocalAndReposition(src, dest, size) => {
                    runtime.stack.copy(
                        frame_start + *src as usize,
                        frame_start + *dest as usize,
                        *size as usize
                    )?;
                    runtime.stack.move_back_ptr(frame_start + *dest as usize + *size as usize)?;
                }
                Instruction::Unimplemented => {
                    return Err(RuntimeError("unimplemented".to_string()))
                }
                Instruction::Test => {
                    let test_value = runtime.stack.pop()?;
                    if unsafe { test_value.primitive().bool() } != false {
                        instruction_idx += 1
                    }
                }
                Instruction::Jump(idx) => {
                    instruction_idx = *idx;
                }
                Instruction::AddI => {
                    let right = runtime.stack.pop()?.primitive().uint();
                    let left = runtime.stack.pop()?.primitive().uint();
                    runtime.stack.push((left + right).into())?;
                }
                Instruction::AddF => {
                    let right = runtime.stack.pop()?.primitive().float();
                    let left = runtime.stack.pop()?.primitive().float();
                    runtime.stack.push((left + right).into())?;
                }
                Instruction::SubI => {
                    let right = runtime.stack.pop()?.primitive().uint();
                    let left = runtime.stack.pop()?.primitive().uint();
                    runtime.stack.push((left - right).into())?;
                }
                Instruction::SubF => {
                    let right = runtime.stack.pop()?.primitive().float();
                    let left = runtime.stack.pop()?.primitive().float();
                    runtime.stack.push((left - right).into())?;
                }
                Instruction::MulI => {
                    let right = runtime.stack.pop()?.primitive().uint();
                    let left = runtime.stack.pop()?.primitive().uint();
                    runtime.stack.push((left * right).into())?;
                }
                Instruction::MulF => {
                    let right = runtime.stack.pop()?.primitive().float();
                    let left = runtime.stack.pop()?.primitive().float();
                    runtime.stack.push((left * right).into())?;
                }
                Instruction::DivU => {
                    let right = runtime.stack.pop()?.primitive().uint();
                    let left = runtime.stack.pop()?.primitive().uint();
                    runtime.stack.push((left / right).into())?;
                }
                Instruction::DivI => {
                    let right = runtime.stack.pop()?.primitive().int();
                    let left = runtime.stack.pop()?.primitive().int();
                    runtime.stack.push((left / right).into())?;
                }
                Instruction::DivF => {
                    let right = runtime.stack.pop()?.primitive().float();
                    let left = runtime.stack.pop()?.primitive().float();
                    runtime.stack.push((left / right).into())?;
                }
                Instruction::Eq(size) => {
                    let right = runtime.stack.pop_many(*size as usize)?;
                    let left = runtime.stack.pop_many(*size as usize)?;

                    runtime.stack.push((left == right).into())?;
                }
                Instruction::Ne(size) => {
                    let right = runtime.stack.pop_many(*size as usize)?;
                    let left = runtime.stack.pop_many(*size as usize)?;

                    runtime.stack.push((left != right).into())?;
                }
                Instruction::LtI => {
                    let right = runtime.stack.pop()?.primitive().int();
                    let left = runtime.stack.pop()?.primitive().int();
                    runtime.stack.push(left.cmp(&right).is_lt().into())?;
                }
                Instruction::LtU => {
                    let right = runtime.stack.pop()?.primitive().uint();
                    let left = runtime.stack.pop()?.primitive().uint();
                    runtime.stack.push(left.cmp(&right).is_lt().into())?;
                }
                Instruction::LtF => {
                    let right = runtime.stack.pop()?.primitive().float();
                    let left = runtime.stack.pop()?.primitive().float();
                    runtime.stack.push(left.cmp(&right).is_lt().into())?;
                }
                Instruction::LeI => {
                    let right = runtime.stack.pop()?.primitive().int();
                    let left = runtime.stack.pop()?.primitive().int();
                    runtime.stack.push(left.cmp(&right).is_le().into())?;
                }
                Instruction::LeU => {
                    let right = runtime.stack.pop()?.primitive().uint();
                    let left = runtime.stack.pop()?.primitive().uint();
                    runtime.stack.push(left.cmp(&right).is_le().into())?;
                }
                Instruction::LeF => {
                    let right = runtime.stack.pop()?.primitive().float();
                    let left = runtime.stack.pop()?.primitive().float();
                    runtime.stack.push(left.cmp(&right).is_le().into())?;
                }
                Instruction::GtI => {
                    let right = runtime.stack.pop()?.primitive().int();
                    let left = runtime.stack.pop()?.primitive().int();
                    runtime.stack.push(left.cmp(&right).is_gt().into())?;
                }
                Instruction::GtU => {
                    let right = runtime.stack.pop()?.primitive().uint();
                    let left = runtime.stack.pop()?.primitive().uint();
                    runtime.stack.push(left.cmp(&right).is_gt().into())?;
                }
                Instruction::GtF => {
                    let right = runtime.stack.pop()?.primitive().float();
                    let left = runtime.stack.pop()?.primitive().float();
                    runtime.stack.push(left.cmp(&right).is_gt().into())?;
                }
                Instruction::GeI => {
                    let right = runtime.stack.pop()?.primitive().int();
                    let left = runtime.stack.pop()?.primitive().int();
                    runtime.stack.push(left.cmp(&right).is_ge().into())?;
                }
                Instruction::GeU => {
                    let right = runtime.stack.pop()?.primitive().uint();
                    let left = runtime.stack.pop()?.primitive().uint();
                    runtime.stack.push(left.cmp(&right).is_ge().into())?;
                }
                Instruction::GeF => {
                    let right = runtime.stack.pop()?.primitive().float();
                    let left = runtime.stack.pop()?.primitive().float();
                    runtime.stack.push(left.cmp(&right).is_ge().into())?;
                }
                Instruction::NegI => {
                    let value = runtime.stack.pop()?.primitive().int();
                    runtime.stack.push((-value).into())?;
                }
                Instruction::NegF => {
                    let value = runtime.stack.pop()?.primitive().float();
                    runtime.stack.push((-value).into())?;
                }
                Instruction::Not => {
                    let value = runtime.stack.pop()?.primitive().uint();
                    runtime.stack.push((!value).into())?;
                }
            }
        }
    }

    fn debug(&self) -> Option<&dyn Debug> {
        Some(self as &dyn Debug)
    }
}