use std::alloc::{alloc, Layout};
use std::collections::LinkedList;
use std::fmt::write;
use std::ops::Range;
use std::ptr::write_volatile;
use crate::common::raw_value::{RawValueReference, Value, ValueArray};
use crate::common::value::{CallableObjectHeader, CompositeObjectHeader, ObjectFlag, ObjectHeader, StringObjectHeader};
use crate::vm::callable::RuntimeError;





pub struct Heap<'obj> {
    composites: LinkedList<&'obj CompositeObjectHeader<'obj>>,
    strings: LinkedList<&'obj StringObjectHeader>,
    callables: LinkedList<&'obj CallableObjectHeader>,
}

impl<'obj> Heap<'obj> {
    pub fn new() -> Self {
        Self {
            composites: LinkedList::new(),
            strings: LinkedList::new(),
            callables: LinkedList::new(),
        }
    }

    pub fn new_composite(&mut self, header: CompositeObjectHeader<'obj>) -> Value<'obj> {
        unsafe {
            let mem = alloc(Layout::new::<CompositeObjectHeader>()) as *mut CompositeObjectHeader<'obj>;
            mem.write(header);
            *(*mem).pointers.get_mut() = 0;
            self.composites.push_front(&*mem);
            Value::new_reference(RawValueReference { composite: mem.as_ref().unwrap() })
        }
    }

    pub fn new_string(&mut self, header: StringObjectHeader) -> Value<'obj> {
        unsafe {
            let mem = alloc(Layout::new::<StringObjectHeader>()) as *mut StringObjectHeader;
            mem.write(header);
            *(*mem).pointers.get_mut() = 0;
            *(*mem).flag.get_mut() = ObjectFlag::New;
            self.strings.push_front(&*mem);
            Value::new_reference(RawValueReference { string: mem.as_ref().unwrap() })
        }
    }

    pub fn new_callable(&mut self, header: CallableObjectHeader) -> Value<'obj> {
        unsafe {
            let mem = alloc(Layout::new::<CallableObjectHeader>()) as *mut CallableObjectHeader;
            mem.write(header);
            *(*mem).pointers.get_mut() = 0;
            self.callables.push_front(&*mem);
            Value::new_reference(RawValueReference { callable: mem.as_ref().unwrap() })
        }
    }
}

// const STACK_SIZE: usize = 2_usize.pow(19);
const STACK_SIZE: usize = 2048;
// const STACK_SIZE: usize = 50;
pub struct Stack<'obj> {
    pub(crate) data: ValueArray<'obj>,
    // stack: Box<[Value<'obj>; STACK_SIZE]>,
    ptr: usize,
}

impl<'obj> Stack<'obj> {
    pub fn new() -> Self {
        Self {
            data: ValueArray::new(STACK_SIZE),
            ptr: 0,
        }
    }

    pub fn move_back_ptr(&mut self, ptr: usize) -> Result<(), RuntimeError> {
        if self.ptr < ptr {
            return Err(RuntimeError("cannot move forward pointer without pushing".to_string()))
        }

        self.data.fill_flag_false(ptr..self.ptr);

        self.ptr = ptr;

        Ok(())
    }

    pub fn get_ptr(&self) -> usize {
        self.ptr
    }

    pub fn push(&mut self, val: Value<'obj>) -> Result<(), RuntimeError> {
        if self.ptr >= self.data.len() {
            return Err(RuntimeError("stack overflow".to_string()))
        }
        self.data.set(self.ptr, val);
        self.ptr += 1;
        Ok(())
    }

    pub fn pop(&mut self) -> Result<Value<'obj>, RuntimeError> {
        if self.ptr == 0 {
            return Err(RuntimeError("stack underflow".to_string()))
        }
        // let popped = self.stack.get(self.ptr-1);
        let popped = self.data.get(self.ptr-1);
        self.move_back_ptr(self.ptr - 1)?;
        Ok(popped)
    }

    pub fn pop_many(&mut self, size: usize) -> Result<ValueArray<'obj>, RuntimeError> {
        if self.ptr < size {
            return Err(RuntimeError("stack underflow".to_string()))
        }

        let values = self.data.get_subset(self.ptr-size..self.ptr);
        self.move_back_ptr(self.ptr - size)?;

        Ok(values)
    }

    pub fn get(&mut self, idx: usize) -> Result<Value<'obj>, RuntimeError> {
        if idx >= self.data.len() {
            return Err(RuntimeError("out of bounds stack read".to_string()))
        }
        Ok(self.data.get(idx))
    }

    pub fn get_many(&mut self, range: Range<usize>) -> Result<ValueArray<'obj>, RuntimeError> {
        if range.end > self.data.len() {
            return Err(RuntimeError("out of bounds stack read".to_string()))
        }

        Ok(self.data.get_subset(range))
    }

    pub fn copy(&mut self, src: usize, dest: usize, size: usize) -> Result<(), RuntimeError> {
        if src < dest {
            if dest+size > self.data.len() {
                return Err(RuntimeError("stack overflow".to_string()))
            }

            for i in (0..size).rev() {
                unsafe {
                    let val = self.data.get_unchecked(src + i);
                    self.data.set_unchecked(dest + i, val);
                }
            }
        } else if src > dest {
            if src+size >= self.data.len() {
                return Err(RuntimeError("out of bounds stack read".to_string()))
            }

            for i in 0..size {
                unsafe {
                    let val = self.data.get_unchecked(src + i);
                    self.data.set_unchecked(dest + i, val);
                }
            }
        }

        Ok(())
    }
}