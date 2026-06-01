use std::fmt::{Debug, Formatter, Pointer, Write};
use std::num::{NonZero, NonZeroUsize};
use std::ops::{Add, BitAnd, Deref, DerefMut, Div, Index, IndexMut, Mul, Neg, Not, Sub};
use std::{mem, ptr};
use std::any::Any;
use std::cell::UnsafeCell;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::mem::ManuallyDrop;
use std::os::linux::raw::stat;
use std::pin::Pin;
use std::ptr::{addr_eq, addr_of, NonNull};
use std::sync::Arc;
use anyhow::Result;
use ordered_float::OrderedFloat;
use crate::common::fsize::{fsize, target_usize};
use crate::vm::callable::PuffinCallable;

#[derive(Debug, Clone)]
pub struct ObjectHeader<T> {
    pub stage: GCStage,
    pub value: T
}

pub type CompositeObjectHeader<'obj> = ObjectHeader<UnsafeCell<Box<Value<'obj>>>>;
pub type StringObjectHeader = ObjectHeader<String>;
pub type CallableObjectHeader = ObjectHeader<Box<dyn PuffinCallable>>;

impl Debug for CallableObjectHeader {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let a = &raw const self;
        match self.debug() {
            Some(debug) => debug.fmt(f),
            None => f.write_fmt(format_args!("PuffinCallable@{:#x}", ptr::from_ref(self).addr()))
        }
    }
}

pub union AmbiguousObjectHeader<'obj> {
    composite: ManuallyDrop<CompositeObjectHeader<'obj>>,
    string: ManuallyDrop<StringObjectHeader>,
    callable: ManuallyDrop<CallableObjectHeader>,
}

impl<T> ObjectHeader<T> {
    pub fn new(stage: GCStage, value: T) -> Self {
        Self { stage, value }
    }
}

#[derive(Debug, Copy, Clone)]
pub enum GCStage {
    QueuedDeletion,
    Preserve,
    Static,
}

// #[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
// #[repr(usize)]
// pub enum ValueMeta {
//     Unit = 0,
//     Bool,
//     UInt,
//     Int,
//     Float,
//     Char,
//     Byte,
//     Composite,
//     String,
//     Function,
// }
//
// #[derive(Clone, Copy)]
// union ValueData {
//     unit: (),
//     bool: bool,
//     uint: usize,
//     int: isize,
//     float: OrderedFloat<fsize>,
//     char: char,
//     byte: u8,
//     composite: *const ObjectHeader,
//     string: *const ObjectHeader,
//     function: *const ObjectHeader,
// }

// #[derive(Clone, Copy)]
// pub struct Value {
//     pub metadata: ValueMeta, // 4-8byte
//     pub data: ValueData, // 4-8byte
// }

#[derive(Clone, Copy, Debug)]
pub enum Value<'obj> {
    Void,
    Bool(bool),
    UInt(usize),
    Int(isize),
    Float(OrderedFloat<fsize>),
    Char(char),
    Byte(u8),
    Composite(&'obj CompositeObjectHeader<'obj>),
    String(&'obj StringObjectHeader),
    Callable(&'obj CallableObjectHeader),
}

impl Hash for Value<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        mem::discriminant(self).hash(state);

        match self {
            Value::Void => ().hash(state),
            Value::Bool(b) => b.hash(state),
            Value::UInt(i) => i.hash(state),
            Value::Int(i) => i.hash(state),
            Value::Float(f) => f.hash(state),
            Value::Char(c) => c.hash(state),
            Value::Byte(i) => i.hash(state),
            Value::Composite(o) => ptr::from_ref(o).hash(state),
            Value::String(o) => o.value.hash(state),
            Value::Callable(o) => ptr::from_ref(o).hash(state),
        }
    }
}

impl Eq for Value<'_> {}

impl PartialEq for Value<'_> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Void, Value::Void) => true,
            (Value::Bool(a), Value::Bool(b)) => a==b,
            (Value::UInt(a), Value::UInt(b)) => a==b,
            (Value::Int(a), Value::Int(b)) => a==b,
            (Value::Float(a), Value::Float(b)) => a==b,
            (Value::Char(a), Value::Char(b)) => a==b,
            (Value::Byte(a), Value::Byte(b)) => a==b,
            (Value::Composite(a), Value::Composite(b)) => addr_eq(a, b),
            (Value::String(a), Value::String(b)) => addr_eq(a, b),
            (Value::Callable(a), Value::Callable(b)) => addr_eq(a, b),
            _ => false
        }
    }
}

impl PartialOrd for Value<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match (self, other) {
            (Value::UInt(a), Value::UInt(b)) => Some(a.cmp(b)),
            (Value::Int(a), Value::Int(b)) => Some(a.cmp(b)),
            (Value::Float(a), Value::Float(b)) => Some(a.cmp(b)),
            (Value::Byte(a), Value::Byte(b)) => Some(a.cmp(b)),
            (Value::Char(a), Value::Char(b)) => Some(a.cmp(b)),
            _ => None
        }
    }
}

impl<'obj> Value<'obj> {
    // pub const UNIT: Value = Self::new(ValueMeta::Unit, ValueData { unit: () } );
    pub const TRUE: Value<'obj> = Self::Bool(true);
    pub const FALSE: Value<'obj> = Self::Bool(false);

    pub fn cast_string(&self) -> String {
        if let Value::String(s) = self {
            s.value.clone()
        } else {
            panic!("could not cast to string")
        }
    }

    pub fn cast_int(&self) -> isize {
        if let Value::Int(i) = self {
            *i
        } else {
            panic!("could not cast to int")
        }
    }

    // pub const fn new(metadata: ValueMeta, data: ValueData) -> Self {
    //     Self { metadata, data }
    // }
    //
    // pub const fn bool(bool: bool) -> Self {
    //     Self::new(ValueMeta::Bool, ValueData { bool } )
    // }
    //
    // unsafe fn get_bool_unchecked(&self) -> bool {
    //     unsafe { self.data.bool }
    // }
    //
    // fn get_bool(&self) -> Option<bool> {
    //     if self.metadata == ValueMeta::Bool {
    //         unsafe { Some(self.get_bool_unchecked()) }
    //     } else {
    //         None
    //     }
    // }
    //
    // pub const fn uint(uint: usize) -> Self {
    //     Self::new(ValueMeta::UInt, ValueData { uint } )
    // }
    //
    // unsafe fn get_uint_unchecked(&self) -> usize {
    //     unsafe { self.data.uint }
    // }
    //
    // pub fn get_uint(&self) -> Option<usize> {
    //     if self.metadata == ValueMeta::UInt {
    //         unsafe { Some(self.get_uint_unchecked()) }
    //     } else {
    //         None
    //     }
    // }
    //
    // pub const fn int(int: isize) -> Self {
    //     Self::new(ValueMeta::Int, ValueData { int } )
    // }
    //
    // unsafe fn get_int_unchecked(&self) -> isize {
    //     unsafe { self.data.int }
    // }
    //
    // pub fn get_int(&self) -> Option<isize> {
    //     if self.metadata == ValueMeta::Int {
    //         unsafe { Some(self.get_int_unchecked()) }
    //     } else {
    //         None
    //     }
    // }
    //
    // pub const fn float(float: OrderedFloat<fsize>) -> Self {
    //     Self::new(ValueMeta::UInt, ValueData { float } )
    // }
    //
    // unsafe fn get_float_unchecked(&self) -> fsize {
    //     unsafe { self.data.float.into() }
    // }
    //
    // pub fn get_float(&self) -> Option<fsize> {
    //     if self.metadata == ValueMeta::Float {
    //         unsafe { Some(self.get_float_unchecked()) }
    //     } else {
    //         None
    //     }
    // }
    //
    // pub const fn char(char: char) -> Self {
    //     Self::new(ValueMeta::UInt, ValueData { char } )
    // }
    //
    // unsafe fn get_char_unchecked(&self) -> char {
    //     unsafe { self.data.char }
    // }
    //
    // pub fn get_char(&self) -> Option<char> {
    //     if self.metadata == ValueMeta::Char {
    //         unsafe { Some(self.get_char_unchecked()) }
    //     } else {
    //         None
    //     }
    // }
    //
    // pub const fn byte(byte: u8) -> Self {
    //     Self::new(ValueMeta::UInt, ValueData { byte } )
    // }
    //
    // unsafe fn get_byte_unchecked(&self) -> u8 {
    //     unsafe { self.data.byte }
    // }
    //
    // pub fn get_byte(&self) -> Option<u8> {
    //     if self.metadata == ValueMeta::Byte {
    //         unsafe { Some(self.get_byte_unchecked()) }
    //     } else {
    //         None
    //     }
    // }
    //
    // pub const fn composite(composite: *const ObjectHeader) -> Self {
    //     Self::new(ValueMeta::Composite, ValueData { composite })
    // }
    //
    // unsafe fn get_composite_unchecked(&self) -> *const ObjectHeader {
    //     unsafe { self.data.composite }
    // }
    //
    // pub fn get_composite(&self) -> Option<*const ObjectHeader> {
    //     if self.metadata == ValueMeta::Composite {
    //         unsafe { Some(self.get_composite_unchecked()) }
    //     } else {
    //         None
    //     }
    // }
    //
    // pub const fn string(string: *const ObjectHeader) -> Self {
    //     Self::new(ValueMeta::String, ValueData { string })
    // }
    //
    // unsafe fn get_string_unchecked(&self) -> *const ObjectHeader {
    //     unsafe { self.data.string }
    // }
    //
    // pub fn get_string(&self) -> Option<*const ObjectHeader> {
    //     if self.metadata == ValueMeta::String {
    //         unsafe { Some(self.get_string_unchecked()) }
    //     } else {
    //         None
    //     }
    // }
    //
    // pub const fn function(function: *const ObjectHeader) -> Self {
    //     Self::new(ValueMeta::Function, ValueData { function })
    // }
    //
    // unsafe fn get_function_unchecked(&self) -> *const ObjectHeader {
    //     unsafe { self.data.function }
    // }
    //
    // pub fn get_function(&self) -> Option<*const ObjectHeader> {
    //     if self.metadata == ValueMeta::Function {
    //         unsafe { Some(self.get_function_unchecked()) }
    //     } else {
    //         None
    //     }
    // }
}

// pub(crate) type ValueTuple<'pool> = Vec<Value<'pool>>;

impl<'obj> Into<Value<'obj>> for usize {
    fn into(self) -> Value<'obj> {
        Value::UInt(self)
    }
}

impl<'obj> Into<Value<'obj>> for () {
    fn into(self) -> Value<'obj> {
        Value::Void
    }
}

impl<'obj> Into<Value<'obj>> for isize {
    fn into(self) -> Value<'obj> {
        Value::Int(self)
    }
}

impl<'obj> Into<Value<'obj>> for fsize {
    fn into(self) -> Value<'obj> {
        Value::Float(self.into())
    }
}

impl<'obj> Into<Value<'obj>> for OrderedFloat<fsize> {
    fn into(self) -> Value<'obj> {
        Value::Float(self)
    }
}

impl<'obj> Into<Value<'obj>> for char {
    fn into(self) -> Value<'obj> {
        Value::Char(self)
    }
}

impl<'obj> Into<Value<'obj>> for u8 {
    fn into(self) -> Value<'obj> {
        Value::Byte(self)
    }
}

impl<'obj> Into<Value<'obj>> for bool {
    fn into(self) -> Value<'obj> {
        Value::Bool(self)
    }
}

// impl<'obj> Into<Value<'obj>> for *const CompositeObjectHeader<'obj> {
//     fn into(self) -> Value<'obj> {
//         Value::Composite(self)
//     }
// }
//
// impl<'obj> Into<Value<'obj>> for *const StringObjectHeader {
//     fn into(self) -> Value<'obj> {
//         Value::String(self)
//     }
// }
//
// impl<'obj> Into<Value<'obj>> for *const CallableObjectHeader {
//     fn into(self) -> Value<'obj> {
//         Value::Callable(self)
//     }
// }

impl<'obj> Into<Value<'obj>> for &'obj CompositeObjectHeader<'obj> {
    fn into(self) -> Value<'obj> {
        Value::Composite(self)
    }
}

impl<'obj> Into<Value<'obj>> for &'obj StringObjectHeader {
    fn into(self) -> Value<'obj> {
        Value::String(self)
    }
}

impl<'obj> Into<Value<'obj>> for &'obj CallableObjectHeader {
    fn into(self) -> Value<'obj> {
        Value::Callable(self)
    }
}

impl<'obj> Add for Value<'obj> {
    type Output = Option<Value<'obj>>;

    fn add(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (Value::UInt(a), Value::UInt(b)) => Some((a+b).into()),
            (Value::Int(a), Value::Int(b)) => Some((a+b).into()),
            (Value::Float(a), Value::Float(b)) => Some((a+b).into()),
            (Value::Byte(a), Value::Byte(b)) => Some((a+b).into()),
            _ => None
        }
    }
}

impl<'obj> Sub for Value<'obj> {
    type Output = Option<Value<'obj>>;

    fn sub(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (Value::UInt(a), Value::UInt(b)) => Some((a-b).into()),
            (Value::Int(a), Value::Int(b)) => Some((a-b).into()),
            (Value::Float(a), Value::Float(b)) => Some((a-b).into()),
            (Value::Byte(a), Value::Byte(b)) => Some((a-b).into()),
            _ => None
        }
    }
}

impl<'obj> Mul for Value<'obj> {
    type Output = Option<Value<'obj>>;

    fn mul(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (Value::UInt(a), Value::UInt(b)) => Some((a*b).into()),
            (Value::Int(a), Value::Int(b)) => Some((a*b).into()),
            (Value::Float(a), Value::Float(b)) => Some((a*b).into()),
            (Value::Byte(a), Value::Byte(b)) => Some((a*b).into()),
            _ => None
        }
    }
}

impl<'obj> Div for Value<'obj> {
    type Output = Option<Value<'obj>>;

    fn div(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (Value::UInt(a), Value::UInt(b)) => Some((a/b).into()),
            (Value::Int(a), Value::Int(b)) => Some((a/b).into()),
            (Value::Float(a), Value::Float(b)) => Some((a/b).into()),
            (Value::Byte(a), Value::Byte(b)) => Some((a/b).into()),
            _ => None
        }
    }
}

impl<'obj> Not for Value<'obj> {
    type Output = Option<Value<'obj>>;

    fn not(self) -> Self::Output {
        match self {
            Value::Bool(v) => Some(v.not().into()),
            Value::UInt(v) => Some(v.not().into()),
            Value::Int(v) => Some(v.not().into()),
            Value::Byte(v) => Some(v.not().into()),
            _ => None
        }
    }
}

impl<'obj> Neg for Value<'obj> {
    type Output = Option<Value<'obj>>;

    fn neg(self) -> Self::Output {
        match self {
            Value::UInt(v) => Some(((v as isize).neg() as usize).into()),
            Value::Int(v) => Some(v.neg().into()),
            Value::Float(v) => Some(v.neg().into()),
            Value::Byte(v) => Some(((v as i8).neg() as u8).into()),
            _ => None
        }
    }
}