use crate::compiler::position::SpanPosition;

#[derive(Debug, Clone)]
pub struct CompilerError {
    pub message: String,
    pub span: SpanPosition,
}