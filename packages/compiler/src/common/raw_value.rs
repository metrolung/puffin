use std::fmt::{Debug, Formatter};
use std::marker::PhantomData;
use std::mem;
use std::mem::MaybeUninit;
use std::ops::{Index, Range};
use ordered_float::OrderedFloat;
use crate::common::value::{AmbiguousObjectHeader, CallableObjectHeader, CompositeObjectHeader, StringObjectHeader};


#[derive(Copy, Clone)]
pub struct Value<'obj> {
    value: RawValue<'obj>,
    flag: bool
}

pub struct StrongValue<'obj>(Value<'obj>);

impl<'obj> StrongValue<'obj> {
    pub fn new(value: Value<'obj>) -> Self {
        let s = Self(value);
        if let Some(r) = s.0.reference() {
            r.ambiguous().watch();
        }
        s
    }

    pub fn primitive(&self) -> RawValuePrimitive {
        unsafe { self.0.value.p }
    }

    pub fn reference(&self) -> Option<RawValueReference<'obj>> {
        if self.0.flag {
            Some(unsafe { self.0.value.r })
        } else {
            None
        }
    }

    pub fn value(&self) -> Value<'obj> {
        self.0
    }
}

impl<'obj> Drop for StrongValue<'obj> {
    fn drop(&mut self) {
        if let Some(r) = self.0.reference() {
            r.ambiguous().unwatch();
        }
    }
}


impl<'obj> Value<'obj> {
    pub fn new_reference(r: RawValueReference<'obj>) -> Self {
        Self {
            value: RawValue { r },
            flag: true
        }
    }

    pub fn new_primitive(p: RawValuePrimitive) -> Self {
        Self {
            value: RawValue { p },
            flag: false
        }
    }

    pub fn primitive(&self) -> RawValuePrimitive {
        unsafe { self.value.p }
    }

    pub fn reference(&self) -> Option<RawValueReference<'obj>> {
        if self.flag {
            Some(unsafe { self.value.r })
        } else {
            None
        }
    }

    pub fn strong(self) -> StrongValue<'obj> {
        StrongValue::new(self)
    }
}

impl<'obj> Into<Value<'obj>> for u64 {
    fn into(self) -> Value<'obj> {
        Value::new_primitive(RawValuePrimitive { uint: self })
    }
}

impl<'obj> Into<Value<'obj>> for i64 {
    fn into(self) -> Value<'obj> {
        Value::new_primitive(RawValuePrimitive { int: self })
    }
}

impl<'obj> Into<Value<'obj>> for f64 {
    fn into(self) -> Value<'obj> {
        Value::new_primitive(RawValuePrimitive { float: self.into() })
    }
}

impl<'obj> Into<Value<'obj>> for bool {
    fn into(self) -> Value<'obj> {
        Value::new_primitive(RawValuePrimitive { bool: self })
    }
}

impl<'obj> Into<Value<'obj>> for char {
    fn into(self) -> Value<'obj> {
        Value::new_primitive(RawValuePrimitive { char: self })
    }
}

impl<'obj> Into<Value<'obj>> for OrderedFloat<f64> {
    fn into(self) -> Value<'obj> {
        Value::new_primitive(RawValuePrimitive { float: self })
    }
}

impl<'obj> Into<Value<'obj>> for &'obj AmbiguousObjectHeader<'obj> {
    fn into(self) -> Value<'obj> {
        Value::new_reference(RawValueReference { ambiguous: self })
    }
}

impl<'obj> Into<Value<'obj>> for &'obj CompositeObjectHeader<'obj> {
    fn into(self) -> Value<'obj> {
        Value::new_reference(RawValueReference { composite: self })
    }
}

impl<'obj> Into<Value<'obj>> for &'obj StringObjectHeader {
    fn into(self) -> Value<'obj> {
        Value::new_reference(RawValueReference { string: self })
    }
}

impl<'obj> Into<Value<'obj>> for &'obj CallableObjectHeader {
    fn into(self) -> Value<'obj> {
        Value::new_reference(RawValueReference { callable: self })
    }
}

impl Debug for Value<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.flag {
            f.write_fmt(format_args!("obj&{}", unsafe { self.value.p.uint }))
        } else {
            f.write_fmt(format_args!("{}", unsafe { self.value.p.uint }))
        }
    }
}

impl PartialEq for Value<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.value.eq(&other.value)
    }
}

impl Eq for Value<'_> {}


#[derive(Copy, Clone)]
pub union RawValuePrimitive {
    uint: u64,
    int: i64,
    float: OrderedFloat<f64>,
    pub char: char,
    pub bool: bool,
}

impl RawValuePrimitive {
    pub fn uint(self) -> u64 {
        unsafe { self.uint }
    }
    pub fn int(self) -> i64 {
        unsafe { self.int }
    }
    pub fn float(self) -> OrderedFloat<f64> {
        unsafe { self.float }
    }
}

impl<'obj> RawValueReference<'obj> {
    pub fn ambiguous(self) -> &'obj AmbiguousObjectHeader<'obj> {
        unsafe { self.ambiguous }
    }
}

impl RawValue<'_> {
    pub fn uint(&self) -> u64 {
        unsafe { self.p.uint }
    }
    pub fn int(self) -> i64 {
        unsafe { self.p.int }
    }
    pub fn float(self) -> OrderedFloat<f64> {
        unsafe { self.p.float }
    }
}

#[derive(Copy, Clone)]
pub union RawValueReference<'obj> {
    pub ambiguous: &'obj AmbiguousObjectHeader<'obj>,
    pub composite: &'obj CompositeObjectHeader<'obj>,
    pub string: &'obj StringObjectHeader,
    pub callable: &'obj CallableObjectHeader,
}

#[derive(Copy, Clone)]
pub union RawValue<'obj> {
    p: RawValuePrimitive,
    r: RawValueReference<'obj>
}

impl PartialEq for RawValue<'_> {
    fn eq(&self, other: &Self) -> bool {
        unsafe { self.p.uint() == other.p.uint() }
    }
}

impl Eq for RawValue<'_> {}

pub struct ValueArray<'obj> {
    data: Box<[RawValue<'obj>]>,
    size: usize,
    _phantom: PhantomData<&'obj ()>
}

impl<'obj> Into<ValueArray<'obj>> for Vec<Value<'obj>> {
    fn into(self) -> ValueArray<'obj> {
        ValueArray::from_slice(&self)
    }
}

const PTR_SIZE: usize = size_of::<RawValue>();
impl<'obj> ValueArray<'obj> {
    pub fn from_slice(values: &[Value<'obj>]) -> Self {
        let mut s = Self::new(values.len());
        for (i, v) in values.iter().enumerate() {
            s.set(i, *v);
        }
        s
    }

    pub fn to_vec(&self) -> Vec<Value<'obj>> {
        let mut vec = vec![];
        for i in 0..self.size {
            vec.push(self.get(i));
        }
        vec
    }

    pub fn to_strong_vec(&self) -> Vec<StrongValue<'obj>> {
        let mut vec = vec![];
        for i in 0..self.size {
            vec.push(self.get(i).strong());
        }
        vec
    }

    pub fn new(size: usize) -> Self {
        Self {
            data: vec![RawValue { p: RawValuePrimitive { uint: 0 } }; size.div_ceil(PTR_SIZE)+size].into_boxed_slice(),
            size,
            _phantom: PhantomData
        }
    }

    fn flags(&self) -> &[RawValue<'obj>] {
        &self.data[0..self.size.div_ceil(PTR_SIZE)]
    }

    fn flags_mut(&mut self) -> &mut [RawValue<'obj>] {
        &mut self.data[0..self.size.div_ceil(PTR_SIZE)]
    }

    fn values(&self) -> &[RawValue<'obj>] {
        let start = self.size.div_ceil(PTR_SIZE);
        unsafe { mem::transmute(&self.data[start..start + self.size]) }
    }

    fn values_mut(&mut self) -> &mut [RawValue<'obj>] {
        let start = self.size.div_ceil(PTR_SIZE);
        unsafe { mem::transmute(&mut self.data[start..start + self.size]) }
    }

    fn set_flag(&mut self, idx: usize, flag: bool) {
        unsafe {
            if flag {
                self.flags_mut()[idx / PTR_SIZE].p.uint |= 1 << idx;
            } else {
                self.flags_mut()[idx / PTR_SIZE].p.uint &= !(1 << idx)
            }
        }
    }

    unsafe fn set_flag_unchecked(&mut self, idx: usize, flag: bool) {
        unsafe {
            if flag {
                self.flags_mut().get_unchecked_mut(idx / PTR_SIZE).p.uint |= 1 << idx;
            } else {
                self.flags_mut().get_unchecked_mut(idx / PTR_SIZE).p.uint &= !(1 << idx)
            }
        }
    }

    // TODO: could be optimized
    fn fill_flag(&mut self, range: Range<usize>, flag: bool) {
        for i in range {
            self.set_flag(i, flag)
        }
    }

    pub fn fill_flag_false(&mut self, range: Range<usize>) {
        self.fill_flag(range, false);
    }

    pub fn get_flag(&self, idx: usize) -> bool {
        let mask = self.flags()[idx/PTR_SIZE].uint();
        (mask >> idx) & 1 == 1
    }

    pub unsafe fn get_flag_unchecked(&self, idx: usize) -> bool {
        unsafe { (self.flags().get_unchecked(idx/PTR_SIZE).uint() >> idx) & 1 == 1 }
    }

    fn set_value(&mut self, idx: usize, value: RawValue<'obj>) {
        self.values_mut()[idx] = value;
    }

    pub unsafe fn set_value_unchecked(&mut self, idx: usize, value: RawValue<'obj>) {
        unsafe { *self.values_mut().get_unchecked_mut(idx) = value; }
    }

    pub fn get_value(&self, idx: usize) -> RawValue<'obj> {
        self.values()[idx]
    }

    pub unsafe fn get_value_unchecked(&self, idx: usize) -> RawValue<'obj> {
        unsafe { *self.values().get_unchecked(idx) }
    }

    // TODO: could be optimized
    pub fn get_subset(&self, range: Range<usize>) -> Self {
        let mut s = Self::new(range.len());
        let range_start = range.start;
        for i in range {
            s.set(i-range_start, self.get(i))
        }
        s
    }

    pub fn get(&self, idx: usize) -> Value<'obj> {
        Value {
            value: self.get_value(idx),
            flag: self.get_flag(idx),
        }
    }

    pub unsafe fn get_unchecked(&self, idx: usize) -> Value<'obj> {
        unsafe {
            Value {
                value: self.get_value_unchecked(idx),
                flag: self.get_flag_unchecked(idx),
            }
        }
    }

    pub fn set(&mut self, idx: usize, value: Value<'obj>) {
        self.set_value(idx, value.value);
        self.set_flag(idx, value.flag);
    }

    pub unsafe fn set_unchecked(&mut self, idx: usize, value: Value<'obj>) {
        unsafe {
            self.set_value_unchecked(idx, value.value);
            self.set_flag_unchecked(idx, value.flag);
        }
    }

    pub fn len(&self) -> usize {
        self.size
    }
}

impl PartialEq for ValueArray<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.values() == other.values()
    }
}

impl Eq for ValueArray<'_> {}