use std::cell::UnsafeCell;
use std::collections::HashMap;
use std::fmt::{Debug, Formatter, Write};
use std::ptr;
use std::rc::Rc;
use crate::common::value::{CallableObjectHeader, GCStage, StringObjectHeader, Value};
use crate::compiler::codegen::Instruction;
use crate::vm::garbage_collector::{ Stack, };

pub struct Runtime<'obj> {
    program: &'obj PuffinProgram,
    stack: Stack<'obj>,
}


// pub trait Linker {
//     fn link(&self, runtime: &mut Runtime, name: &str) -> Option<Value>;
// }
//
// pub struct HashLinker {
//     map: HashMap<String, Value>
// }
//
// impl HashLinker {
//     pub fn new() -> Self {
//         Self { map: HashMap::new() }
//     }
//
//     pub fn link(&mut self, key: String, value: Value) {
//         self.map.insert(key, value);
//     }
// }

// impl Linker for HashLinker {
//     fn link(&self, runtime: &mut Runtime, name: &str) -> Option<Value> {
//         self.map.get(name).copied()
//     }
// }

pub trait PuffinCallable {
    fn invoke(&self, runtime: &mut Runtime, frame_start: usize) -> Result<(), RuntimeError>;
    fn debug(&self) -> Option<&dyn Debug> { None }
}

#[derive(Debug)]
pub struct RuntimeError(pub String);

// pub struct Invoker<'a, 'r> {
//     runtime: &'a mut Runtime<'a, 'r>,
//     static_pool: &'a GCPool<'r>,
//     frame_start: usize
// }
//

pub struct Invoker<'runtime, 'obj> {
    runtime: &'runtime mut Runtime<'obj>,
    frame_start: usize
}
//
// impl<'runtime, 'obj> Invoker<'runtime, 'obj> {
//     pub fn invoke(&mut self, value: &dyn PuffinCallable) -> Result<Vec<Value<'obj>>, RuntimeError> {
//         value.invoke(self.runtime, self.frame_start)?;
//
//         let mut returns = vec![Value::Unit; self.runtime.stack.ptr];
//         self.runtime.stack.get_many(0, &mut returns)?;
//         Ok(returns)
//     }
// }

// type NativeFunctionFunctionType<'runtime, 'obj> = fn(
//     fn(
//         invoker: Invoker<'runtime, 'obj>,
//         params: Vec<Value<'obj>>
//     ) -> Result<Vec<Value<'obj>>, RuntimeError>
// );
//




// #[derive(Debug)]
// pub struct NativeFunction {
//     function: for<'runtime, 'obj> fn(
//         invoker: Invoker<'runtime, 'obj>,
//         params: Vec<Value<'obj>>
//     ) -> Result<Vec<Value<'obj>>, RuntimeError>,
// }
//
// impl NativeFunction {
//     pub fn new(
//         function: for<'runtime, 'obj> fn(
//             invoker: Invoker<'runtime, 'obj>,
//             params: Vec<Value<'obj>>
//         ) -> Result<Vec<Value<'obj>>, RuntimeError>
//     ) -> Self {
//         Self { function }
//     }
// }
//

pub enum FunctionHelper {}

impl FunctionHelper {
    pub fn new(callable: impl for <'obj> Fn(Invoker<'_, 'obj>, Vec<Value<'obj>>) -> Result<Vec<Value<'obj>>, RuntimeError> + 'static) -> CallableObjectHeader {
        CallableObjectHeader::new(
            GCStage::Static,
            Box::new(callable) as Box<dyn PuffinCallable>
        )
    }
}

// type VariadicFunction<T> where T: impl for<'runtime, 'obj> Fn(
//     invoker: Invoker<'runtime, 'obj>,
//     params: Vec<Value<'obj>>
// ) -> Result<Vec<Value<'obj>>, RuntimeError> = T;

impl<T> PuffinCallable for T
where T: for <'obj> Fn(Invoker<'_, 'obj>, Vec<Value<'obj>>) -> (Result<Vec<Value<'obj>>, RuntimeError>) {
    fn invoke(&self, runtime: &mut Runtime, frame_start: usize) -> Result<(), RuntimeError> {
        let mut buf = vec![Value::Void; runtime.stack.get_ptr()-frame_start];
        runtime.stack.get_many(frame_start, &mut buf)?;
        runtime.stack.move_back_ptr(frame_start)?;
        
        let invoker = Invoker {
            runtime,
            frame_start
        };

        let result = self(invoker, buf)?;

        for value in result {
            runtime.stack.push(value)?;
        }

        Ok(())
    }
}

impl PuffinCallable for Value<'_> {
    fn invoke(&self, runtime: &mut Runtime, frame_start: usize) -> Result<(), RuntimeError> {
        let Value::Callable(callable) = self else {
            return Err(RuntimeError("value is not function".to_string()))
        };

        callable.invoke(runtime, frame_start)
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
        let function_idx = self.function_lookup.get(&name).ok_or(RuntimeError("could not find function".to_string()))?;
        self.function_table[*function_idx] = callable;

        Ok(())
    }

    pub fn execute<'obj>(&'obj self, name: &str) -> Result<Vec<Value<'obj>>, RuntimeError> {
        let mut puffin_thread: Runtime<'obj> = Runtime {
            program: &self,
            stack: Stack::new(),
        };

        let function_idx = self.function_lookup.get(name).ok_or(RuntimeError("could not find function".to_string()))?;
        let function = &self.function_table[*function_idx];
        function.value.invoke(&mut puffin_thread, 0)?;

        let mut returns = vec![Value::Void; puffin_thread.stack.get_ptr()];
        puffin_thread.stack.get_many(0, &mut returns)?;

        Ok(returns)
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

            instruction_idx += 1;

            match instruction {
                Instruction::LoadInt(i) => {
                    runtime.stack.push((*i).into())?
                }
                Instruction::LoadUInt(i) => {
                    runtime.stack.push((*i).into())?
                }
                Instruction::LoadVoid => {
                    runtime.stack.push(().into())?
                }
                Instruction::LoadFloat(f) => {
                    runtime.stack.push((*f).into())?
                }
                Instruction::LoadChar(c) => {
                    runtime.stack.push((*c).into())?
                }
                Instruction::LoadBool(b) => {
                    runtime.stack.push((*b).into())?
                }
                Instruction::LoadByte(i) => {
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
                    let mut buf = vec![Value::Void; *size as usize];
                    runtime.stack.get_many(adr, &mut buf)?;
                    for value in buf {
                        runtime.stack.push(value)?;
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
                    if test_value != Value::Bool(false) {
                        instruction_idx += 1
                    }
                }
                Instruction::Jump(idx) => {
                    instruction_idx = *idx;
                }
                Instruction::Add => {
                    let right = runtime.stack.pop()?;
                    let left = runtime.stack.pop()?;
                    runtime.stack.push((left + right).ok_or(RuntimeError("cannot be applied to types".to_string()))?)?;
                }
                Instruction::Sub => {
                    let right = runtime.stack.pop()?;
                    let left = runtime.stack.pop()?;
                    runtime.stack.push((left - right).ok_or(RuntimeError("cannot be applied to types".to_string()))?)?;
                }
                Instruction::Mul => {
                    let right = runtime.stack.pop()?;
                    let left = runtime.stack.pop()?;
                    runtime.stack.push((left * right).ok_or(RuntimeError("cannot be applied to types".to_string()))?)?;
                }
                Instruction::Div => {
                    let right = runtime.stack.pop()?;
                    let left = runtime.stack.pop()?;
                    runtime.stack.push((left / right).ok_or(RuntimeError("cannot be applied to types".to_string()))?)?;
                }
                Instruction::Eq(size) => {
                    let mut left = vec![Value::Void; *size as usize];
                    runtime.stack.pop_many(&mut left)?;
                    let mut right = vec![Value::Void; *size as usize];
                    runtime.stack.pop_many(&mut right)?;

                    runtime.stack.push((left == right).into())?;
                }
                Instruction::Ne(size) => {
                    let mut left = vec![Value::Void; *size as usize];
                    runtime.stack.pop_many(&mut left)?;
                    let mut right = vec![Value::Void; *size as usize];
                    runtime.stack.pop_many(&mut right)?;

                    runtime.stack.push((left != right).into())?;
                }
                Instruction::Lt => {
                    let right = runtime.stack.pop()?;
                    let left = runtime.stack.pop()?;
                    runtime.stack.push(left.partial_cmp(&right).ok_or(RuntimeError("cannot be applied to types".to_string()))?.is_lt().into())?;
                }
                Instruction::Le => {
                    let right = runtime.stack.pop()?;
                    let left = runtime.stack.pop()?;
                    runtime.stack.push(left.partial_cmp(&right).ok_or(RuntimeError("cannot be applied to types".to_string()))?.is_le().into())?;
                }
                Instruction::Gt => {
                    let right = runtime.stack.pop()?;
                    let left = runtime.stack.pop()?;
                    runtime.stack.push(left.partial_cmp(&right).ok_or(RuntimeError("cannot be applied to types".to_string()))?.is_gt().into())?;
                }
                Instruction::Ge => {
                    let right = runtime.stack.pop()?;
                    let left = runtime.stack.pop()?;
                    runtime.stack.push(left.partial_cmp(&right).ok_or(RuntimeError("cannot be applied to types".to_string()))?.is_ge().into())?;
                }
                Instruction::Neg => {
                    let value = runtime.stack.pop()?;
                    runtime.stack.push((-value).ok_or(RuntimeError("cannot be applied to type".to_string()))?)?;
                }
                Instruction::Not => {
                    let value = runtime.stack.pop()?;
                    runtime.stack.push((!value).ok_or(RuntimeError("cannot be applied to type".to_string()))?)?;
                }
            }
        }
    }

    fn debug(&self) -> Option<&dyn Debug> {
        Some(self as &dyn Debug)
    }
}