use std::alloc::{alloc, Layout};
use std::fmt::write;
use std::ptr::write_volatile;
use crate::common::value::{ObjectHeader, Value};
use crate::vm::callable::RuntimeError;




// pub struct Pool {
//
// }


// struct GcNode(*const ObjectHeader, *const GcNode);


// struct TraceRoot {
//     // node: GcNode,
// }

// struct ValuePool<const SIZE: usize> {
//     values: [Value; SIZE]
// }

// #[derive(Debug, Default)]
// pub struct StaticPool {
//     // pub pool: MemPool
// }
//
// #[derive(Default)]
// pub struct GarbageCollector {
//     // heap: MemPool
// }
//
// impl GarbageCollector {
//     fn new() -> Self {
//         Self {
//             heap: MemPool::default()
//         }
//     }
// }

// const STACK_SIZE: usize = 2_usize.pow(19);
const STACK_SIZE: usize = 2048;
// const STACK_SIZE: usize = 50;
pub struct Stack<'obj> {
    stack: Box<[Value<'obj>; STACK_SIZE]>,
    ptr: usize,
}

impl<'obj> Stack<'obj> {
    pub fn new() -> Self {
        Self {
            stack: Box::new([Value::Void; STACK_SIZE]),
            ptr: 0,
        }
    }

    pub fn move_back_ptr(&mut self, ptr: usize) -> Result<(), RuntimeError> {
        if self.ptr < ptr {
            return Err(RuntimeError("cannot move forward pointer without pushing".to_string()))
        }

        for i in ptr..self.ptr {
            self.stack[i] = Value::Void;
        }

        self.ptr = ptr;

        Ok(())
    }

    pub fn get_ptr(&self) -> usize {
        self.ptr
    }

    pub fn push(&mut self, val: Value<'obj>) -> Result<(), RuntimeError> {
        if self.ptr >= self.stack.len() {
            return Err(RuntimeError("stack overflow".to_string()))
        }
        self.stack[self.ptr] = val;
        self.ptr += 1;
        Ok(())
    }

    pub fn pop(&mut self) -> Result<Value<'obj>, RuntimeError> {
        if self.ptr == 0 {
            return Err(RuntimeError("stack underflow".to_string()))
        }
        let popped = self.stack[self.ptr-1];
        self.move_back_ptr(self.ptr - 1)?;
        Ok(popped)
    }

    pub fn pop_many(&mut self, buf: &mut [Value<'obj>]) -> Result<(), RuntimeError> {
        if self.ptr < buf.len() {
            return Err(RuntimeError("stack underflow".to_string()))
        }

        buf.copy_from_slice(&self.stack[self.ptr-buf.len()..self.ptr]);
        self.move_back_ptr(self.ptr - buf.len())?;

        Ok(())
    }

    pub fn get(&mut self, idx: usize) -> Result<Value<'obj>, RuntimeError> {
        if idx >= self.stack.len() {
            return Err(RuntimeError("out of bounds stack read".to_string()))
        }
        Ok(self.stack[idx])
    }

    pub fn get_many(&mut self, idx: usize, buf: &mut [Value<'obj>]) -> Result<(), RuntimeError> {
        if idx+buf.len() > self.stack.len() {
            return Err(RuntimeError("out of bounds stack read".to_string()))
        }

        buf.copy_from_slice(&self.stack[idx..idx + buf.len()]);

        Ok(())
    }

    pub fn copy(&mut self, src: usize, dest: usize, size: usize) -> Result<(), RuntimeError> {
        if src < dest {
            if dest+size > self.stack.len() {
                return Err(RuntimeError("stack overflow".to_string()))
            }

            for i in (0..size).rev() {
                unsafe {
                    let val = self.stack.get_unchecked(src + i);
                    *self.stack.get_unchecked_mut(dest + i) = *val
                }
            }
        } else if src > dest {
            if src+size >= self.stack.len() {
                return Err(RuntimeError("out of bounds stack read".to_string()))
            }

            for i in 0..size {
                unsafe {
                    let val = self.stack.get_unchecked(src + i);
                    *self.stack.get_unchecked_mut(dest + i) = *val
                }
            }
        }

        Ok(())
    }
}