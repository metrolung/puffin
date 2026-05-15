use std::fmt::{Debug, Formatter, Pointer};
use std::num::{NonZero, NonZeroUsize};
use std::ops::{Add, BitAnd, Deref, Index, IndexMut};
use std::{mem, ptr};
use std::any::Any;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::os::linux::raw::stat;
use std::ptr::{addr_eq, NonNull};
use std::sync::Arc;
use anyhow::Result;
use ordered_float::OrderedFloat;
use crate::common::fsize::{fsize, target_usize};


pub struct Object {
    pub gc_stage: GcStage,
    kind: ObjectKind,
}

enum ObjectKind {
    Object(Vec<Value>),
    // Userdata(Box<dyn UserdataSupport>),
    Array(ValueMeta, Vec<ValueData>),
    String(String),
}

impl Debug for ObjectKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ObjectKind::Object(values) => {
                f.write_str("object")
            }
            ObjectKind::Array(meta, _) => {
                f.write_fmt(format_args!("{:?}[]", meta))
            }
            ObjectKind::String(string) => {
                f.write_str(string)
            }
        }
    }
}

impl Object {
    pub fn string(string: String, gc_stage: GcStage) -> Self {
        Self {
            kind: ObjectKind::String(string),
            gc_stage,
        }
    }

    pub fn object(object: Vec<Value>, gc_stage: GcStage) -> Self {
        Self {
            kind: ObjectKind::Object(object),
            gc_stage,
        }
    }
}



#[derive(Debug, Copy, Clone)]
pub enum GcStage {
    QueuedDeletion,
    Preserve,
    Static,
}

impl Debug for Object {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.kind.fmt(f)
    }
}


#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
#[repr(usize)]
pub enum ValueMeta {
    Unit,
    Bool,
    UInt,
    Int,
    Float,
    Char,
    Byte,
    Object,
}

#[derive(Clone, Copy)]
union ValueData {
    unit: (),
    bool: bool,
    uint: usize,
    int: isize,
    float: OrderedFloat<fsize>,
    char: char,
    byte: u8,
    object: *mut Object,
    native: fn(),
}

#[derive(Clone, Copy)]
pub struct Value {
    pub metadata: ValueMeta, // 4-8byte
    data: ValueData, // 4-8byte
}

impl Hash for Value {
    fn hash<H: Hasher>(&self, state: &mut H) {
        unsafe {
            state.write_usize(self.metadata.clone() as usize);
            state.write_usize(self.data.uint)
        }
    }
}

impl Debug for Value {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self.metadata {
            ValueMeta::Unit => f.write_str("unit"),
            ValueMeta::Bool => self.get_bool().fmt(f),
            ValueMeta::UInt => self.get_uint().fmt(f),
            ValueMeta::Int => self.get_int().fmt(f),
            ValueMeta::Float => self.get_float().fmt(f),
            ValueMeta::Char => self.get_char().fmt(f),
            ValueMeta::Byte => self.get_byte().fmt(f),
            ValueMeta::Object => Debug::fmt(&self.get_object(), f),
        }
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        unsafe { self.metadata == other.metadata && self.data.uint == other.data.uint }
    }
}
impl Eq for Value {}

impl Value {
    pub const UNIT: Value = Self::new(ValueMeta::Unit, ValueData { unit: () } );
    pub const TRUE: Value = Self::new(ValueMeta::Bool, ValueData { bool: true });
    pub const FALSE: Value = Self::new(ValueMeta::Bool, ValueData { bool: false });

    pub const fn new(metadata: ValueMeta, data: ValueData) -> Self {
        Self { metadata, data }
    }

    pub const fn bool(bool: bool) -> Self {
        Self::new(ValueMeta::Bool, ValueData { bool } )
    }

    unsafe fn get_bool_unchecked(&self) -> bool {
        unsafe { self.data.bool }
    }

    fn get_bool(&self) -> bool {
        if self.metadata == ValueMeta::Bool {
            unsafe { self.get_bool_unchecked() }
        } else {
            panic!("Failed cast of Value into usize")
        }
    }

    pub const fn uint(uint: usize) -> Self {
        Self::new(ValueMeta::UInt, ValueData { uint } )
    }

    unsafe fn get_uint_unchecked(&self) -> usize {
        unsafe { self.data.uint }
    }

    fn get_uint(&self) -> usize {
        if self.metadata == ValueMeta::UInt {
            unsafe { self.get_uint_unchecked() }
        } else {
            panic!("Failed cast of Value into usize")
        }
    }

    pub const fn int(int: isize) -> Self {
        Self::new(ValueMeta::Int, ValueData { int } )
    }

    unsafe fn get_int_unchecked(&self) -> isize {
        unsafe { self.data.int }
    }

    fn get_int(&self) -> isize {
        if self.metadata == ValueMeta::Int {
            unsafe { self.get_int_unchecked() }
        } else {
            panic!("Failed cast of Value into isize")
        }
    }

    pub const fn float(float: OrderedFloat<fsize>) -> Self {
        Self::new(ValueMeta::UInt, ValueData { float } )
    }

    unsafe fn get_float_unchecked(&self) -> fsize {
        unsafe { self.data.float.into() }
    }

    fn get_float(&self) -> fsize {
        if self.metadata == ValueMeta::Float {
            unsafe { self.get_float_unchecked() }
        } else {
            panic!("Failed cast of Value into fsize")
        }
    }

    pub const fn char(char: char) -> Self {
        Self::new(ValueMeta::UInt, ValueData { char } )
    }

    unsafe fn get_char_unchecked(&self) -> char {
        unsafe { self.data.char }
    }

    fn get_char(&self) -> char {
        if self.metadata == ValueMeta::Char {
            unsafe { self.get_char_unchecked() }
        } else {
            panic!("Failed cast of Value into char")
        }
    }

    pub const fn byte(byte: u8) -> Self {
        Self::new(ValueMeta::UInt, ValueData { byte } )
    }

    unsafe fn get_byte_unchecked(&self) -> u8 {
        unsafe { self.data.byte }
    }

    fn get_byte(&self) -> u8 {
        if self.metadata == ValueMeta::Byte {
            unsafe { self.get_byte_unchecked() }
        } else {
            panic!("Failed cast of Value into byte")
        }
    }

    pub const fn object(object: *mut Object) -> Self {
        Self::new(ValueMeta::Object, ValueData { object })
    }

    unsafe fn get_object_unchecked(&self) -> *mut Object {
        unsafe { self.data.object }
    }

    fn get_object(&self) -> *mut Object {
        if self.metadata == ValueMeta::Object {
            unsafe { self.get_object_unchecked() }
        } else {
            panic!("Failed cast of Value into object")
        }
    }
}

pub(crate) type ValueTuple = Vec<Value>;

impl Into<Value> for usize {
    fn into(self) -> Value {
        Value::uint(self)
    }
}

impl Into<Value> for isize {
    fn into(self) -> Value {
        Value::int(self)
    }
}

impl Into<Value> for fsize {
    fn into(self) -> Value {
        Value::float(self.into())
    }
}

impl Into<Value> for OrderedFloat<fsize> {
    fn into(self) -> Value {
        Value::float(self)
    }
}

impl Into<Value> for char {
    fn into(self) -> Value {
        Value::char(self)
    }
}

impl Into<Value> for u8 {
    fn into(self) -> Value {
        Value::byte(self)
    }
}

impl Into<Value> for bool {
    fn into(self) -> Value {
        Value::bool(self)
    }
}

impl Add for Value {
    type Output = Result<Value>;

    fn add(self, other: Value) -> Self::Output {
        match (&self.metadata, &other.metadata) {
            (ValueMeta::Int, ValueMeta::Int) => {
                Ok((self.get_int() + other.get_int()).into())
            }
            (ValueMeta::UInt, ValueMeta::UInt) => {
                Ok((self.get_uint() + other.get_uint()).into())
            }
            (ValueMeta::Float, ValueMeta::Float) => {
                Ok((self.get_float() + other.get_float()).into())
            }
            _ => anyhow::bail!("Cannot add types"),
        }
    }
}

// impl Value {
//     pub fn get(&self, prop: Value) -> Option<Value> {
//         if let ValueData::Object(obj) = self.data {
//             unsafe {
//                 (obj.as_ref().map_section).get()
//             }
//         } else {
//             None
//         }
//     }
// }
