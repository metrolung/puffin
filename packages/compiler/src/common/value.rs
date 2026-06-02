use std::fmt::{Debug, Formatter, Pointer, Write};
use std::{mem, ptr};
use std::cell::UnsafeCell;
use std::mem::ManuallyDrop;
use std::sync::atomic::AtomicUsize;
use crate::common::raw_value::ValueArray;
use crate::vm::callable::PuffinCallable;

#[derive(Debug)]
pub struct ObjectHeader<T> {
    pub flag: UnsafeCell<ObjectFlag>,
    pub pointers: UnsafeCell<usize>,
    pub value: T
}

pub type CompositeObjectHeader<'obj> = ObjectHeader<UnsafeCell<ValueArray<'obj>>>;
pub type StringObjectHeader = ObjectHeader<String>;
pub type CallableObjectHeader = ObjectHeader<Box<dyn PuffinCallable>>;
pub type AmbiguousObjectHeader<'obj> = ObjectHeader<AmbiguousObject<'obj>>;

impl Debug for CallableObjectHeader {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self.debug() {
            Some(debug) => debug.fmt(f),
            None => f.write_fmt(format_args!("PuffinCallable@{:#x}", ptr::from_ref(self).addr()))
        }
    }
}

pub union AmbiguousObject<'obj> {
    composite: ManuallyDrop<UnsafeCell<ValueArray<'obj>>>,
    string: ManuallyDrop<String>,
    callable: ManuallyDrop<Box<dyn PuffinCallable>>,
}

impl<T> ObjectHeader<T> {
    pub fn new(value: T) -> Self {
        Self {
            flag: UnsafeCell::new(ObjectFlag::QueueFree),
            pointers: UnsafeCell::new(0),
            value,
        }
    }

    pub fn set_flag(&self, flag: ObjectFlag) {
        unsafe {
            *self.flag.get() = flag;
        }
    }

    pub fn watch(&self) {
        unsafe { *self.pointers.get() += 1 };
    }

    pub fn unwatch(&self) {
        unsafe { *self.pointers.get() -= 1 };
    }
}

#[derive(Debug, Copy, Clone)]
pub enum ObjectFlag {
    QueueFree,
    Preserve,
    Static
}

impl Into<StringObjectHeader> for String {
    fn into(self) -> StringObjectHeader {
        ObjectHeader::new(self)
    }
}