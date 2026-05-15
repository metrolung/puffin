use std::cmp::Ordering;

pub trait Position {
    fn start(&self) -> PinPosition;
    fn end(&self) -> PinPosition;
    fn span<T: Position>(&self, other: &T) -> SpanPosition {
        SpanPosition {
            first: self.start(),
            last: other.end(),
        }
    }
    fn to_span(&self) -> SpanPosition {
        SpanPosition {
            first: self.start(),
            last: self.end(),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct PinPosition {
    pub row: usize,
    pub col: usize,
    pub idx: usize,
}

impl PartialOrd for PinPosition {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.idx.partial_cmp(&other.idx)
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct SpanPosition {
    pub first: PinPosition,
    pub last: PinPosition,
}

impl SpanPosition {
    pub fn len(&self) -> usize {
        self.last.idx - self.first.idx + 1
    }
}

impl Position for PinPosition {
    fn start(&self) -> PinPosition {
        *self
    }

    fn end(&self) -> PinPosition {
        *self
    }
}

impl Position for SpanPosition {
    fn start(&self) -> PinPosition {
        self.first
    }

    fn end(&self) -> PinPosition {
        self.last
    }
}