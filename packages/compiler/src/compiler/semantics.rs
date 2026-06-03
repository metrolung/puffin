use crate::compiler::parser::{AST, Statement, StatementKind, ValueExpr, ValueExprKind, Implementation, BinOpKind, TypeExpr, TypeExprKind, UnOpKind};
use std::cell::{Ref, RefCell, RefMut};
use std::collections::HashMap;
use std::fmt::{Debug, Display, Formatter};
use std::mem;
use std::ops::{Add, DerefMut, Index};
use std::pin::Pin;
use std::process::{ExitCode, Termination};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::thread::scope;
use anyhow::bail;
use corosensei::{Coroutine, CoroutineResult, Yielder};
// use corosensei::{Coroutine, Yielder};
use crate::common::value::{ ObjectHeader};

use crate::compiler::error::CompilerError;
use crate::compiler::position::{Position, SpanPosition};
use crate::compiler::namespace::{Namespace, Path};

// #[derive(Debug)]
// pub struct Struct {
//     path: Path,
// }

pub type StructFields = HashMap<String, (Offset, Type)>;
pub type StructRef = Path;


#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Type {
    Struct(StructRef),
    Tuple(Vec<Type>),
    Callable(Vec<Type>, Box<Type>),
    Int,
    UInt,
    Float,
    Char,
    Str,
    Bool,
    Void,
    Value,
    Never,
}

impl Type {
    pub fn unit() -> Self {
        Self::Tuple(vec![])
    }

    pub fn is_never(&self) -> bool {
        matches!(self, Type::Never)
    }

    pub fn is_unit(&self) -> bool {
        self.eq(&Type::unit())
    }

    pub fn is_value(&self) -> bool {
        self.eq(&Type::Value)
    }

    pub fn get_with_index(&self, i: usize) -> Option<(Type, Offset)> {
        match self {
            Type::Tuple(types) => {
                let mut sum = 0;
                for i in 0..i {
                    sum += types[i].get_size();
                }
                Some((types[i].clone(), sum))
            }
            _ => None
        }
    }

    pub fn get_size(&self) -> Offset {
        match self {
            Type::Tuple(params) => {
                let mut sum = 0;
                for param in params {
                    sum += param.get_size();
                }
                sum
            },
            Type::Never => 0,
            _ => 1,
        }
    }
}

// struct Field {
//     index: usize,
//     type_: Type,
//     name: String,
// }

type FunctionIdx = usize;
type ConstIdx = usize;
type Offset = u32;

#[derive(Debug, Clone)]
enum SemanticValue {
    LocalVariable(Type, Offset),
    ObjectVariable(Type, Offset),
    ConstVariable(Type, ConstIdx),
    // Expr(TypedValueExpr, Type), // the second type is the possible returning type, not the type of the value returned by TypedExpr

    Type(Type),
    // Struct(StructRef),
    StructFields(StructFields),

    Function {
        param_types: Vec<Type>,
        return_type: Type,
        function_idx: FunctionIdx
    }
}

#[derive(Debug, Clone)]
pub enum TypedBinOpKind {
    AddI, AddF,
    SubI, SubF,
    MulI, MulF,
    DivI, DivU, DivF,
    Eq, Ne,
    LtI, LtU, LtF,
    LeI, LeU, LeF,
    GtI, GtU, GtF,
    GeI, GeU, GeF
}

#[derive(Debug, Clone)]
pub enum TypedUnOpKind {
    NegI, NegF,
    Not
}

#[derive(Debug, Clone)]
pub enum TypedValueExprKind {
    // Value(Value),
    // String(String),
    LitStr(String),
    Primitive(u64),
    // Unreachable,
    Invoke(Box<TypedValueExpr>, Vec<TypedValueExpr>),
    Tuple(Vec<TypedValueExpr>),

    ObjectAccess(Box<TypedValueExpr>, Offset),
    LocalAccess(Offset),

    TupleAccess(
        Box<TypedValueExpr>,
        Offset,
    ),

    ConstVariable(ConstIdx),
    GetFunction(FunctionIdx),
    // AssignVariable(Box<TypedExpr>),
    ReassignVariable(Offset, Box<TypedValueExpr>),
    BinOp(TypedBinOpKind, Box<TypedValueExpr>, Box<TypedValueExpr>),
    UnOp(TypedUnOpKind, Box<TypedValueExpr>),
    Return(Option<Box<TypedValueExpr>>),
    Block(
        Vec<TypedValueExpr>,
        Option<Box<TypedValueExpr>>,
        // tail_expr: Option<Box<TypedExpr>>
    ),
    NoOpCast(Box<TypedValueExpr>),
    If(Box<TypedValueExpr>, Box<TypedValueExpr>, Option<Box<TypedValueExpr>>),
}

#[derive(Debug, Clone)]
pub struct TypedValueExpr {
    pub kind: TypedValueExprKind,
    pub span: SpanPosition,
    pub type_: Type,
    pub place: Offset,
}

impl TypedValueExprKind {
    fn expr(self, span: SpanPosition, type_: Type, place: Offset) -> TypedValueExpr {
        TypedValueExpr {
            kind: self,
            span,
            type_,
            place
        }
    }   
}

#[derive(Debug)]
struct SemanticContext {
    namespace: Namespace<SemanticValue>,
    resulting_ast: TypedAST,
    errors: Vec<CompilerError>,
}

impl SemanticContext {
    fn set(&mut self, path: Path, value: SemanticValue) {
        self.namespace.set(path, value);
    }

    fn is_defined(&self, path: &Path) -> bool {
        self.get(path).is_some()
    }

    fn get(&self, path: &Path) -> Option<&SemanticValue> {
        self.namespace.get(path)
    }

    fn get_mut(&mut self, path: &Path) -> Option<&mut SemanticValue> {
        self.namespace.get_mut(path)
    }
}

impl Index<&Path> for SemanticContext {
    type Output = SemanticValue;

    fn index(&self, index: &Path) -> &Self::Output {
        self.get(index)
            .expect(&format!("variable '{}' is not defined", index))
    }
}

enum DefinitionStateKind {
    Resume,
    PendingDefinition(Path, String),
    DoNotResume(CompilerError),
    Finished,
}

struct DefinitionState {
    generator: Coroutine<(), DefinitionStateKind, DefinitionStateKind>,
    kind: DefinitionStateKind,
}


#[derive(Debug)]
pub enum TypedDefinitionKind {
    Function(TypedValueExpr),
    Constant(TypedValueExpr)
}

#[derive(Debug)]
pub struct TypedDefinition {
    kind: TypedDefinitionKind,
    span: SpanPosition,
}

#[derive(Debug)]
pub struct TypedFunction {
    pub export_name: Option<String>,
    pub body: Option<TypedValueExpr>
}

#[derive(Debug)]
pub struct TypedAST {
    pub constants: Vec<TypedValueExpr>,
    pub functions: Vec<TypedFunction>,
}

struct DefinitionPass<'pass> {
    ctx: Rc<RefCell<SemanticContext>>,
    statement: Statement,
    yielder: &'pass Yielder<(), DefinitionStateKind>
}

fn get_common_type(pass: &DefinitionPass, span: &SpanPosition, type_a: Type, type_b: Type) -> Type {
    if can_cast_to(pass, &type_a, &type_b) {
        return type_b
    }

    if can_cast_to(pass, &type_b, &type_a) {
        return type_a
    }

    error(pass, &format!("no common type between {:?} and {:?} is found", type_a, type_b), span)
}

fn can_cast_to(pass: &DefinitionPass, type_a: &Type, type_b: &Type) -> bool {
    if type_a == type_b {
        return true;
    }

    if type_b.is_value() {
        match type_a {
            Type::Struct(_)
            | Type::Callable(_, _)
            | Type::Int
            | Type::UInt
            | Type::Float
            | Type::Char
            | Type::Str
            | Type::Bool
            | Type::Void => return true,
            _ => ()
        }

        if type_a.is_unit() {
            return true
        }
    }

    if type_a.is_never() {
        return true;
    }

    false
}

fn cast(pass: &DefinitionPass, span: &SpanPosition, expr: TypedValueExpr, ty: Type) -> TypedValueExpr {
    let place = expr.place;

    if expr.type_ == ty {
        return expr;
    }

    if ty.is_value() {
        match expr.type_ {
            Type::Struct(_)
            | Type::Callable(_, _)
            | Type::Int
            | Type::UInt
            | Type::Float
            | Type::Char
            | Type::Str
            | Type::Bool
            | Type::Void => return expr,
            _ => ()
        }

        if expr.type_.is_unit() {
            return TypedValueExprKind::Primitive(0).expr(*span, ty, place)
        }
    }

    if expr.type_.is_never() {
        return TypedValueExprKind::NoOpCast(Box::new(expr)).expr(*span, ty, place);
    }

    error(pass, &format!("could not cast {:?} to {:?}", expr.type_, ty), span);
}

fn find_or_yield(pass: &DefinitionPass, path: Path, name: String) -> SemanticValue {
    if let Some(info) = find(&mut borrow_ctx(pass), path.clone(), name.clone()) {
        return info;
    }

    pass.yielder.suspend(DefinitionStateKind::PendingDefinition(
        path.clone(),
        name.clone(),
    ));
    find(&mut borrow_ctx(pass), path, name).unwrap()
}

fn set(pass: &DefinitionPass, path: Path, value: SemanticValue) {
    borrow_ctx(pass).set(path, value)
}

fn get(pass: &DefinitionPass, name: &Path) -> Option<SemanticValue> {
    borrow_ctx(pass).get(name).cloned()
}

fn find(ctx: &mut SemanticContext, mut path: Path, name: String) -> Option<SemanticValue> {
    loop {
        if let Some(x) = ctx.get(&path.subpath(name.clone())) {
            return Some(x.clone());
        }

        if path.is_root() {
            break;
        }

        path = path.get_parent().unwrap();
    }

    None
}

fn borrow_ctx<'a>(pass: &'a DefinitionPass) -> RefMut<'a, SemanticContext> {
    pass.ctx.borrow_mut()
}

fn visit_type_expr(
    pass: &DefinitionPass,
    path: Path,
    expr: &TypeExpr
) -> Type {
    let span = expr.span.clone();

    match &expr.kind {
        TypeExprKind::Variable(name) => {
            let value = find_or_yield(pass, path, name.clone());

            match value {
                SemanticValue::Type(ty) => ty.clone(),
                _ => error(pass, "expected type", &span),
            }
        }
        TypeExprKind::Tuple(exprs) => {
            let mut types = vec![];

            for expr in exprs {
                types.push(visit_type_expr(pass, path.clone(), expr));
            }

            Type::Tuple(types)
        }
    }
}

fn visit_value_expr(
    pass: &DefinitionPass,
    path: Path,
    return_type: Option<&Type>,
    scope_expr_idx: &mut usize,
    local_var_idx: &mut Offset,
    expr: &ValueExpr
) -> TypedValueExpr {
    let place = *local_var_idx;
    *scope_expr_idx += 1;

    let span = expr.span.clone();
    match &expr.kind {
        ValueExprKind::Variable(name) => {
            let value = find_or_yield(pass, path, name.clone());

            match value {
                // SemanticValue::Expr(expr, return_type, ..) => (expr, return_type),
                SemanticValue::Function { param_types, return_type, function_idx } => {
                    *local_var_idx += 1;
                    TypedValueExprKind::GetFunction(function_idx)
                         .expr(
                             span.clone(),
                             Type::Callable(param_types, Box::new(return_type)),
                             place
                         )
                },
                SemanticValue::LocalVariable(type_, idx) => {
                    *local_var_idx += type_.get_size();
                    TypedValueExprKind::LocalAccess(idx)
                         .expr(
                             span.clone(),
                             type_,
                             place
                         )
                }
                SemanticValue::ConstVariable(type_, idx) => {
                    *local_var_idx += type_.get_size();
                    TypedValueExprKind::ConstVariable(idx)
                         .expr(
                             span.clone(),
                             type_,
                             place
                         )
                }
                _ => error(pass, "expected value", &span),
            }
        }
        ValueExprKind::Tuple(exprs) => {
            let mut typed_exprs = vec![];

            for expr in exprs {
                let typed_expr = visit_value_expr(pass, path.clone(), return_type, scope_expr_idx, local_var_idx, expr);
                typed_exprs.push(typed_expr);
            }

            let type_ = Type::Tuple(typed_exprs.iter().map(|a| a.type_.clone()).collect());

            TypedValueExprKind::Tuple(typed_exprs)
                .expr(span, type_, place)
        }
        ValueExprKind::LitInt(i) => {
            *local_var_idx += 1;
            TypedValueExprKind::Primitive(*i as u64)
                .expr(span, Type::Int, place)
        }
        ValueExprKind::LitUInt(i) => {
            *local_var_idx += 1;
            TypedValueExprKind::Primitive(*i)
                .expr(span, Type::UInt, place)
        }
        ValueExprKind::LitFloat(i) => {
            *local_var_idx += 1;
            TypedValueExprKind::Primitive(f64::to_bits(*i))
                .expr(span, Type::Float, place)
        }
        ValueExprKind::LitChar(i) => {
            *local_var_idx += 1;
            TypedValueExprKind::Primitive(*i as u64)
                .expr(span, Type::Char, place)
        }
        ValueExprKind::LitBool(i) => {
            *local_var_idx += 1;
            TypedValueExprKind::Primitive(*i as u8 as u64)
                .expr(span, Type::Bool, place)
        }
        ValueExprKind::LitStr(s) => {
            *local_var_idx += 1;
            TypedValueExprKind::LitStr(s.clone())
                .expr(span, Type::Str, place)
        }
        ValueExprKind::Return(value) => {
            let Some(return_type) = return_type else {
                error(pass, "cannot return in this context", &span);
            };

            if let Some(value) = value {
                let value = visit_value_expr(pass, path, Some(return_type), scope_expr_idx, local_var_idx, value);
                TypedValueExprKind::Return(
                    Some(Box::new(cast(
                        pass,
                        &span,
                        value,
                        return_type.clone()
                    )))
                ).expr(span, Type::Never, place)
            } else {
                if !return_type.is_unit() {
                    error(pass, "please specify return value", &span);
                }

                TypedValueExprKind::Return(None).expr(span, Type::Never, place)
            }
        }
        ValueExprKind::AssignVariable(name, ty, right) => {
            let var_path = path.subpath(name.clone());

            let right = visit_value_expr(
                pass,
                path.clone(),
                return_type,
                scope_expr_idx,
                local_var_idx,
                right
            );

            if let Some(ty) = &ty {
                let ty = visit_type_expr(pass, path, ty);

                set(
                    pass,
                    var_path,
                    SemanticValue::LocalVariable(ty.clone(), right.place),
                );

                cast(pass, &span, right, ty)
            } else {
                set(
                    pass,
                    var_path,
                    SemanticValue::LocalVariable(right.type_.clone(), right.place),
                );

                right
            }
        }
        ValueExprKind::Block(exprs, tail) => {
            let branch_path = path.subpath(format!("$scope{}", scope_expr_idx));
            let mut statements = vec![];
            let mut scope_expr_idx = 0;
            let mut block_var_idx = local_var_idx.clone();

            for expr in exprs {
                let expr = visit_value_expr(
                    pass,
                    branch_path.clone(),
                    return_type,
                    &mut scope_expr_idx,
                    &mut block_var_idx,
                    expr,
                );

                if expr.type_.is_never() {
                    statements.push(expr);
                    return TypedValueExprKind::Block(statements, None).expr(span, Type::Never, place);
                } else {
                    statements.push(expr);
                };
            }

            if let Some(tail) = tail {
                let tail = visit_value_expr(
                    pass,
                    branch_path.clone(),
                    return_type,
                    &mut scope_expr_idx,
                    &mut block_var_idx,
                    tail,
                );

                let tail_type = tail.type_.clone();

                *local_var_idx += tail_type.get_size();

                TypedValueExprKind::Block(statements, Some(Box::new(tail))).expr(span, tail_type, place)
            } else {
                TypedValueExprKind::Block(statements, None).expr(span, Type::unit(), place)
            }
        }
        ValueExprKind::ReassignVariable(left, right) => {
            todo!()
            // let (left, left_return_value) = visit_expr_expect_expr(pass, path.clone(), scope_expr_idx, local_var_idx, left);
            // let (right, right_return_value) = visit_expr_expect_expr(pass, path.clone(), scope_expr_idx, local_var_idx, right);
            //
            // let return_type = get_common_type(pass, &span, &left_return_value, &right_return_value);
            //
            // if is_same_type(pass, &left.type_, &right.type_) {
            //     error(pass, "expected lhs and rhs to be same type", &span)
            // }
            //
            // match left.kind {
            //     //
            //     // // SemanticValue::LocalVariable(type_, idx) => {
            //     // //     SemanticValue::Expr(
            //     // //         TypedExprKind::ReassignVariable(idx, Box::new(right))
            //     // //             .expr(span, type_),
            //     // //         return_value
            //     // //     )
            //     // // }
            //     // // _ => todo!()
            //     // TypedExprKind::TupleAccess(left, indexed_offset) => {
            //     //     match left.kind {
            //     //
            //     //         TypedExprKind::LocalAccess(offset) => {
            //     //
            //     //         }
            //     //     }
            //     //
            //     //     SemanticValue::Expr(
            //     //         TypedExprKind::ReassignVariable(idx, Box::new(right))
            //     //             .expr(span, Type::Never),
            //     //         return_type
            //     //     )
            //     // }
            //     // // TypedExprKind::ObjectAccess(_, _) => {
            //     // //
            //     // // }
            //     // TypedExprKind::LocalAccess(offset) => {
            //     //     SemanticValue::Expr(
            //     //         TypedExprKind::ReassignVariable(offset, Box::new(right))
            //     //             .expr(span, Type::Never),
            //     //         return_type
            //     //     )
            //     // }
            //
            //     TypedExprKind::LocalAccess(offset) => {
            //         SemanticValue::Expr(
            //             TypedExprKind::LocalAccess(offset+indexed_offset)
            //                 .expr(span, indexed_type),
            //             return_type
            //         )
            //     }
            //     TypedExprKind::ObjectAccess(expr, offset) => {
            //         SemanticValue::Expr(
            //             TypedExprKind::ObjectAccess(expr, offset+indexed_offset)
            //                 .expr(span, indexed_type),
            //             return_type
            //         )
            //     }
            //
            //     _ => error(pass, "invalid left hand side", &span)
            // }
        }
        ValueExprKind::IntegerAccess(left, index) => {
            let left = visit_value_expr(pass, path.clone(), return_type, scope_expr_idx, &mut local_var_idx.clone(), left);

            let Some((indexed_type, indexed_offset)) = left.type_.get_with_index(*index) else {
                error(pass, "expected tuple", &span)
            };

            *local_var_idx += indexed_type.get_size();
            match left.kind {
                TypedValueExprKind::LocalAccess(offset) =>
                    TypedValueExprKind::LocalAccess(offset + indexed_offset).expr(span, indexed_type, place),
                _ =>
                    TypedValueExprKind::TupleAccess(Box::new(left), indexed_offset).expr(span, indexed_type, place)
            }
        }
        ValueExprKind::IdentifierAccess(left, ident) => {
            let left = visit_value_expr(pass, path.clone(), return_type, scope_expr_idx, local_var_idx, left);

            if let Type::Struct(struct_ref) = &left.type_ {
                let fields = find_or_yield(pass, struct_ref.clone(), "$fields".to_string());
                let SemanticValue::StructFields(fields) = fields else {
                    error(pass, "expected struct to have fields", &span);
                };

                let Some((indexed_offset, indexed_type)) = fields.get(ident) else {
                    error(pass, &format!("object does not have field `{}`", ident), &span)
                };

                let indexed_type = indexed_type.clone();

                TypedValueExprKind::ObjectAccess(
                    Box::new(left),
                    *indexed_offset
                ).expr(span, indexed_type, place)
            } else {
                error(pass, "cannot index a non-struct", &span)
            }
        }
        ValueExprKind::Call(left, params) => {
            let mut call_var_idx = *local_var_idx;

            let left = visit_value_expr(pass, path.clone(), return_type, scope_expr_idx, &mut call_var_idx, left);
            let Type::Callable(param_types, function_return_type) = left.type_.clone() else {
                error(pass, "expected callable", &span)
            };

            if param_types.len() != params.len() {
                error(pass, &format!("expected {} parameters, found {}", param_types.len(), params.len()), &span);
            }

            let mut passed_typed_params = vec![];
            for i in 0..params.len() {
                let param = visit_value_expr(pass, path.clone(), return_type, scope_expr_idx, &mut call_var_idx, &params[i]);
                let ty = param_types[i].clone();
                passed_typed_params.push(cast(pass, &param.span.clone(), param, ty));
            }

            *local_var_idx += function_return_type.get_size();

            TypedValueExprKind::Invoke(
                Box::new(left),
                passed_typed_params
            ).expr(span, *function_return_type, place)
        }
        ValueExprKind::BinOp(op_kind, left, right) => {
            let mut param_var_idx = local_var_idx.clone();
            let left = visit_value_expr(pass, path.clone(), return_type, scope_expr_idx, &mut param_var_idx, left);
            let right = visit_value_expr(pass, path.clone(), return_type, scope_expr_idx, &mut param_var_idx, right);

            let (op_kind, result_type) = match (op_kind, &left.type_, &right.type_) {
                (BinOpKind::Add, Type::Int, Type::Int) => (TypedBinOpKind::AddI, Type::Int),
                (BinOpKind::Add, Type::UInt, Type::UInt) => (TypedBinOpKind::AddI, Type::UInt),
                (BinOpKind::Add, Type::Float, Type::Float) => (TypedBinOpKind::AddI, Type::Float),

                (BinOpKind::Sub, Type::Int, Type::Int) => (TypedBinOpKind::SubI, Type::Int),
                (BinOpKind::Sub, Type::UInt, Type::UInt) => (TypedBinOpKind::SubI, Type::UInt),
                (BinOpKind::Sub, Type::Float, Type::Float) => (TypedBinOpKind::SubF, Type::Float),

                (BinOpKind::Mul, Type::Int, Type::Int) => (TypedBinOpKind::MulI, Type::Int),
                (BinOpKind::Mul, Type::UInt, Type::UInt) => (TypedBinOpKind::MulI, Type::UInt),
                (BinOpKind::Mul, Type::Float, Type::Float) => (TypedBinOpKind::MulF, Type::Float),

                (BinOpKind::Div, Type::Int, Type::Int) => (TypedBinOpKind::DivI, Type::Int),
                (BinOpKind::Div, Type::UInt, Type::UInt) => (TypedBinOpKind::DivU, Type::UInt),
                (BinOpKind::Div, Type::Float, Type::Float) => (TypedBinOpKind::DivF, Type::Float),

                (BinOpKind::Lt, Type::Int, Type::Int) => (TypedBinOpKind::LtI, Type::Int),
                (BinOpKind::Lt, Type::UInt, Type::UInt) => (TypedBinOpKind::LtU, Type::UInt),
                (BinOpKind::Lt, Type::Float, Type::Float) => (TypedBinOpKind::LtF, Type::Float),

                (BinOpKind::Le, Type::Int, Type::Int) => (TypedBinOpKind::LeI, Type::Int),
                (BinOpKind::Le, Type::UInt, Type::UInt) => (TypedBinOpKind::LeU, Type::UInt),
                (BinOpKind::Le, Type::Float, Type::Float) => (TypedBinOpKind::LeF, Type::Float),

                (BinOpKind::Gt, Type::Int, Type::Int) =>     (TypedBinOpKind::GtI, Type::Int),
                (BinOpKind::Gt, Type::UInt, Type::UInt) =>   (TypedBinOpKind::GtU, Type::UInt),
                (BinOpKind::Gt, Type::Float, Type::Float) => (TypedBinOpKind::GtF, Type::Float),

                (BinOpKind::Ge, Type::Int, Type::Int) =>     (TypedBinOpKind::GeI, Type::Int),
                (BinOpKind::Ge, Type::UInt, Type::UInt) =>   (TypedBinOpKind::GeU, Type::UInt),
                (BinOpKind::Ge, Type::Float, Type::Float) => (TypedBinOpKind::GeF, Type::Float),

                (BinOpKind::Eq, _, _) => (TypedBinOpKind::Eq, Type::Bool),
                (BinOpKind::Ne, _, _) => (TypedBinOpKind::Ne, Type::Bool),
                (a, b, c) => error(pass, &format!("cannot perform binary operation `{:?}` on {:?} and {:?}", a, b, c), &span)
            };

            *local_var_idx += result_type.get_size();
            TypedValueExprKind::BinOp(
                op_kind.clone(),
                Box::new(left),
                Box::new(right),
            ).expr(span, result_type, place)
        }
        ValueExprKind::UnOp(op_kind, value) => {
            let value = visit_value_expr(pass, path.clone(), return_type, scope_expr_idx, &mut local_var_idx.clone(), value);

            let (op_kind, result_type) = match (op_kind, &value.type_) {
                (UnOpKind::Not, Type::Int) => (TypedUnOpKind::Not, Type::Int),
                (UnOpKind::Not, Type::UInt) => (TypedUnOpKind::Not, Type::UInt),
                (UnOpKind::Not, Type::Float) => (TypedUnOpKind::Not, Type::Float),
                (UnOpKind::Not, Type::Bool) => (TypedUnOpKind::Not, Type::Bool),

                (UnOpKind::Neg, Type::Int) => (TypedUnOpKind::NegI, Type::Int),
                (UnOpKind::Neg, Type::Float) => (TypedUnOpKind::NegF, Type::Float),

                (UnOpKind::Pos, Type::Int) => return value,
                (UnOpKind::Pos, Type::UInt) => return value,
                (UnOpKind::Pos, Type::Float) => return value,

                (a, b) => error(pass, &format!("cannot perform unary operation `{:?}` on {:?}", a, b), &span)
            };
            *local_var_idx += result_type.get_size();
            TypedValueExprKind::UnOp(
                op_kind,
                Box::new(value),
            ).expr(span, result_type, place)
        }
        ValueExprKind::If(cond, main_branch, else_branch) => {
            let cond = visit_value_expr(pass, path.clone(), return_type, scope_expr_idx, &mut local_var_idx.clone(), cond);

            if cond.type_ != Type::Bool {
                error(pass, &format!("condition must be boolean, not {:?}", cond.type_), &span)
            }

            let branch_path = path.subpath(format!("$branch{}", scope_expr_idx));
            let main_branch = visit_value_expr(pass, branch_path, return_type, scope_expr_idx, &mut local_var_idx.clone(), main_branch);

            let span = cond.span.span(&main_branch.span);

            if let Some(else_branch) = else_branch {
                let branch_path = path.subpath(format!("$branch{}", scope_expr_idx));
                let else_branch = visit_value_expr(pass, branch_path, return_type, scope_expr_idx, &mut local_var_idx.clone(), else_branch);

                let span = cond.span.span(&else_branch.span);

                let common_type = get_common_type(pass, &span, main_branch.type_.clone(), else_branch.type_.clone());

                *local_var_idx += common_type.get_size();

                TypedValueExprKind::If(
                    Box::new(cond),
                    Box::new(cast(pass, &main_branch.span.clone(), main_branch, common_type.clone())),
                    Some(Box::new(cast(pass, &else_branch.span.clone(), else_branch, common_type.clone())))
                ).expr(span, common_type, place)
            } else {
                if !main_branch.type_.is_unit() && !main_branch.type_.is_never() {
                    error(pass, "if statement without else branch must evaluate to unit or never", &span)
                }

                TypedValueExprKind::If(Box::new(cond), Box::new(main_branch), None)
                    .expr(span, Type::unit(), place)
            }
        }
        ValueExprKind::Cast(value, ty) => {
            let left = visit_value_expr(pass, path.clone(), return_type, scope_expr_idx, local_var_idx, value);
            let ty = visit_type_expr(pass, path.clone(), ty);

            cast(pass, &span, left, ty)
        }
    }
}


fn error(pass: &DefinitionPass, message: &str, span: &SpanPosition) -> ! {
    pass.yielder
        .suspend(DefinitionStateKind::DoNotResume(CompilerError {
            message: message.to_string(),
            span: span.clone(),
        }));
    unreachable!()
}

fn statement(pass: &DefinitionPass, path: Path) -> Result<(), CompilerError> {
    let span = &pass.statement.span;

    match &pass.statement.kind {
        StatementKind::ConstStatement {
            name,
            const_type,
            value: right,
        } => {
            let var_path = path.subpath(name.clone());

            let right = visit_value_expr(
                pass,
                path.clone(),
                None,
                &mut 0,
                &mut 0,
                right
            );

            let left_type = visit_type_expr(
                pass,
                path,
                const_type
            );

            if right.type_ != left_type {
                error(pass, "type mismatch", span);
            }

            let right_type = right.type_.clone();

            let definition_idx = {
                let mut ctx = borrow_ctx(pass);
                let definition_idx = ctx.resulting_ast.constants.len();

                ctx.resulting_ast.constants.push(right);
                definition_idx
            };

            set(pass, var_path, SemanticValue::ConstVariable(right_type, definition_idx))
        }
        StatementKind::StructStatement { name, fields, .. } => {
            let struct_path = path.subpath(name.clone());
            set(
                pass,
                struct_path.clone(),
                SemanticValue::Type(Type::Struct(struct_path.clone())),
            );

            let mut fields_processed = HashMap::new();

            for (i, (field_name, field_type)) in fields.iter().enumerate() {
                let left = visit_type_expr(
                    pass,
                    path.clone(),
                    field_type
                );

                fields_processed.insert(field_name.clone(), (i as Offset, left));
            }

            set(
                pass,
                struct_path.subpath("$fields".to_string()),
                SemanticValue::StructFields(fields_processed),
            );
        }
        StatementKind::FunctionStatement {
            name,
            return_type,
            param_bindings,
            statement,
            export,
        } => {
            let func_path = path.subpath(name.clone());
            let export_name = if *export {
                Some(func_path.to_string())
            } else {
                None
            };

            let return_type = if let Some(expr) = &return_type {
                visit_type_expr(pass, path.clone(), expr)
            } else {
                Type::unit()
            };

            let function_idx = borrow_ctx(pass).resulting_ast.functions.len();

            let mut local_var_idx: Offset = 0;

            let mut param_types = vec![];
            for (param_name, param_type) in param_bindings {
                let param_type = visit_type_expr(
                    pass,
                    path.clone(),
                    param_type
                );
                let param_path = func_path.subpath(param_name.clone());

                let param_idx = local_var_idx;
                local_var_idx += param_type.get_size();

                set(pass, param_path.clone(), SemanticValue::LocalVariable(param_type.clone(), param_idx));
                param_types.push(param_type);
            }

            set(pass, func_path.clone(), SemanticValue::Function {
                param_types,
                return_type: return_type.clone(),
                function_idx
            });

            let func = if let Some(statement) = statement {
                let statement = visit_value_expr(
                    pass,
                    func_path.clone(),
                    Some(&return_type),
                    &mut 0,
                    &mut local_var_idx,
                    statement
                );

                TypedFunction {
                    export_name,
                    body: Some(statement)
                }
            } else {
                TypedFunction {
                    export_name,
                    body: None
                }
            };

            borrow_ctx(pass).resulting_ast.functions.push(func);
        }
        _ => todo!(),
    }

    Ok(())
}

trait CoroutineResultImpl<T> {
    fn get_value(self) -> T;
}

impl<T> CoroutineResultImpl<T> for CoroutineResult<T, T> {
    fn get_value(self) -> T {
        match self {
            CoroutineResult::Yield(val) => val,
            CoroutineResult::Return(val) => val,
        }
    }
}

pub fn semantic_check(tree: AST) -> Result<TypedAST, Vec<CompilerError>> {
    let ctx = Rc::new(RefCell::new(SemanticContext {
        namespace: Namespace::new(),
        resulting_ast: TypedAST {
            constants: vec![],
            functions: vec![],
        },
        errors: vec![],
    }));

    {
        let mut ctx = ctx.borrow_mut();

        ctx.set(
            Path::ROOT.subpath("int".to_string()),
            SemanticValue::Type(Type::Int)
        );
        ctx.set(
            Path::ROOT.subpath("uint".to_string()),
            SemanticValue::Type(Type::UInt)
        );
        ctx.set(
            Path::ROOT.subpath("float".to_string()),
            SemanticValue::Type(Type::Float)
        );
        ctx.set(
            Path::ROOT.subpath("char".to_string()),
            SemanticValue::Type(Type::Char)
        );
        ctx.set(
            Path::ROOT.subpath("str".to_string()),
            SemanticValue::Type(Type::Str)
        );
        ctx.set(
            Path::ROOT.subpath("bool".to_string()),
            SemanticValue::Type(Type::Bool)
        );
        ctx.set(
            Path::ROOT.subpath("value".to_string()),
            SemanticValue::Type(Type::Value)
        );
    }

    let mut definition_states = vec![];

    for def in tree.definitions {
        let ctx_clone = Rc::clone(&ctx);

        let generator = Coroutine::new(|yielder, _| {
            let err = statement(
                &DefinitionPass {
                    ctx: ctx_clone,
                    statement: def,
                    yielder: &yielder
                },
                Path::ROOT.subpath("package".to_string())
            );

            if let Err(err) = err {
                DefinitionStateKind::DoNotResume(err)
            } else {
                DefinitionStateKind::Finished
            }
        });

        definition_states.push(DefinitionState {
            generator,
            kind: DefinitionStateKind::Resume,
        });
    }

    let mut errors = vec![];

    loop {
        let mut possibly_frozen = true;
        let mut possibly_finished = true;

        for state in definition_states.iter_mut() {
            match &state.kind {
                DefinitionStateKind::Resume => {
                    possibly_frozen = false;
                    possibly_finished = false;
                    state.kind = state.generator.resume(()).get_value();
                }
                DefinitionStateKind::PendingDefinition(path, name) => {
                    possibly_finished = false;
                    if find(&mut ctx.borrow_mut(), path.clone(), name.clone()).is_some() {
                        possibly_frozen = false;
                        state.kind = state.generator.resume(()).get_value();
                    }
                }
                DefinitionStateKind::DoNotResume(err) => {
                    errors.push(err.clone());
                    state.kind = DefinitionStateKind::Finished
                }
                DefinitionStateKind::Finished => {}
            }
        }

        if possibly_finished {
            break;
        }

        if possibly_frozen {
            errors.push(CompilerError {
                message: "Circular resolution could not be solved".to_string(),
                span: SpanPosition::DUMMY,
            });

            break;
        }
    }

    let mut ctx = ctx.borrow_mut();

    //let namespace = &ctx.namespace;

    //println!("{:#?}", namespace);

    if !errors.is_empty() {
        return Err(errors);
    }

    let mut ast = TypedAST {
        constants: vec![],
        functions: vec![],
    };

    mem::swap(&mut ast, &mut ctx.resulting_ast);

    Ok(ast)
}
