use crate::compiler::parser::{AST, Binding, Definition, DefinitionKind, Expr, ExprKind, Implementation};
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

use corosensei::{Coroutine, CoroutineResult, Yielder};
// use corosensei::{Coroutine, Yielder};
use crate::common::value::{GcStage, Object, Value, ValueMeta};

use crate::compiler::error::CompilerError;
use crate::compiler::position::SpanPosition;
use enum_assoc::Assoc;
use crate::common::fsize::fsize;
use crate::compiler::namespace::{Namespace, Path};

// #[derive(Debug)]
// pub struct Struct {
//     path: Path,
// }

pub type StructRef = Path;

#[derive(Clone, Debug)]
struct StandardTypes {
    int: StructRef,
    uint: StructRef,
    float: StructRef,
    char: StructRef,
    byte: StructRef,
    str: StructRef,
    void: StructRef,
    bool: StructRef,
}

#[derive(Debug, Clone)]
pub enum Type {
    StructObject(StructRef),
    Tuple(Vec<Type>),
    Never,
}

impl Type {
    pub fn is_never(&self) -> bool {
        matches!(self, Type::Never)
    }

    pub fn get_offset_of_index(&self, i: usize) -> Option<usize> {
        match self {
            Type::StructObject(_) => None,
            Type::Tuple(types) => {
                let mut sum = 0;
                for i in 0..i {
                    sum += types[i].get_size();
                }
                Some(sum)
            }
            Type::Never => None
        }
    }

    pub fn get_size(&self) -> usize {
        match self {
            Type::StructObject(_) => 1,
            Type::Tuple(params) => {
                let mut sum = 0;
                for param in params {
                    sum += param.get_size();
                }
                sum
            },
            Type::Never => 0,
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
type LocalIdx = usize;

#[derive(Debug, Clone)]
enum SemanticValue {
    LocalVariable(Type, LocalIdx),
    ConstVariable(Type, ConstIdx),
    Expr(TypedExpr, Type),

    Struct(StructRef),
    StructFields(HashMap<String, (usize, Type)>),

    Function(Type, FunctionIdx),
}

#[derive(Debug, Clone)]
pub enum TypedExprKind {
    // Value(Value),
    // String(String),
    LitStr(String),
    LitInt(isize),
    LitUInt(usize),
    LitFloat(fsize),
    LitByte(u8),
    LitChar(char),
    LitBool(bool),
    LitUnit,
    Tuple(Vec<TypedExpr>),
    IdentifierAccess(Box<TypedExpr>, String),
    IntegerAccess(Box<TypedExpr>, usize),
    LocalVariable(LocalIdx),
    ConstVariable(ConstIdx),
    AssignVariable(Box<TypedExpr>),
    Return(Box<TypedExpr>),
    Block(Vec<TypedExpr>, Option<Box<TypedExpr>>),
}

#[derive(Debug, Clone)]
pub struct TypedExpr {
    pub kind: TypedExprKind,
    pub span: SpanPosition,
    pub type_: Type,
}

impl TypedExprKind {
    fn expr(self, span: SpanPosition, type_: Type) -> TypedExpr {
        TypedExpr {
            kind: self,
            span,
            type_,
        }
    }   
}

#[derive(Debug)]
struct SemanticContext {
    namespace: Namespace<SemanticValue>,
    statics: Vec<Pin<Box<Object>>>,
    resulting_ast: TypedAST,
    standard: StandardTypes,
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
    Function(TypedExpr),
    Constant(TypedExpr)
}

#[derive(Debug)]
pub struct TypedDefinition {
    kind: TypedDefinitionKind,
    span: SpanPosition,
}

#[derive(Debug)]
pub struct TypedAST {
    pub constants: Vec<TypedExpr>,
    pub functions: Vec<TypedExpr>,
}

struct DefinitionPass<'pass> {
    ctx: Rc<RefCell<SemanticContext>>,
    def: Definition,
    yielder: &'pass Yielder<(), DefinitionStateKind>,
}

fn standard_types() -> StandardTypes {
    let int = Path::ROOT.subpath("int".to_string());
    let uint = Path::ROOT.subpath("uint".to_string());
    let float = Path::ROOT.subpath("float".to_string());
    let str = Path::ROOT.subpath("str".to_string());
    let byte = Path::ROOT.subpath("byte".to_string());
    let char = Path::ROOT.subpath("char".to_string());
    let void = Path::ROOT.subpath("void".to_string());
    let bool = Path::ROOT.subpath("bool".to_string());

    StandardTypes {
        int,
        uint,
        float,
        str,
        byte,
        char,
        void,
        bool,
    }
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

fn get_common_type(
    pass: &DefinitionPass,
    span: &SpanPosition,
    type_a: &Type,
    type_b: &Type,
) -> Type {
    match (type_a, type_b) {
        (Type::Never, _) => type_b.clone(),
        (_, Type::Never) => type_a.clone(),
        (Type::StructObject(struct_a), Type::StructObject(struct_b)) => {
            if struct_a == struct_b {
                type_a.clone()
            } else {
                error(pass, "incompatible types", span)
            }
        }
        _ => error(pass, "incompatible types", span),
    }
}

fn is_same_type(pass: &DefinitionPass, type_a: &Type, type_b: &Type) -> bool {
    match (type_a, type_b) {
        (Type::Never, Type::Never) => true,
        (Type::StructObject(struct_a), Type::StructObject(struct_b)) => struct_a == struct_b,
        (Type::Tuple(contents_a), Type::Tuple(contents_b)) => {
            if contents_a.len() != contents_b.len() {
                false
            } else {
                for i in 0..contents_a.len() {
                    if !is_same_type(pass, &contents_a[i], &contents_b[i]) {
                        return false
                    }
                }
                true
            }
        }
        _ => false,
    }
}

fn can_cast_to(pass: &DefinitionPass, subtype: &Type, super_type: &Type) -> bool {
    is_same_type(pass, subtype, super_type) || is_subtype_of(pass, subtype, super_type)
}

fn is_subtype_of(pass: &DefinitionPass, subtype: &Type, super_type: &Type) -> bool {
    match (super_type, subtype) {
        (_, Type::Never) => true,
        _ => false,
    }
}

fn unit(pass: &DefinitionPass, span: SpanPosition) -> SemanticValue {
    SemanticValue::Expr(
        TypedExprKind::LitUnit.expr(span, unit_type(pass)),
        Type::Never,
    )
}

fn unit_type(pass: &DefinitionPass) -> Type {
    Type::StructObject(borrow_ctx(pass).standard.void.clone())
}

fn visit_expr_expect_expr(
    pass: &DefinitionPass,
    path: Path,
    scope_expr_idx: &mut usize,
    local_var_idx: &mut usize,
    expr: &Expr,
) -> (TypedExpr, Type) {
    let span = &expr.span;
    let expr = visit_expr(pass, path, scope_expr_idx, local_var_idx, expr);
    match expr {
        SemanticValue::Expr(expr, return_type) => (expr, return_type),
        _ => error(pass, "expected expression", span),
    }
}

fn visit_expr_expect_type(
    pass: &DefinitionPass,
    path: Path,
    expr: &Expr
) -> Type {
    let span = &expr.span;
    let expr = visit_expr(pass, path.subpath("$typecheck".to_string()), &mut 0, &mut 0, expr);
    match expr {
        SemanticValue::Struct(struct_ref) => Type::StructObject(struct_ref),
        _ => error(pass, "expected type", span),
    }
}

fn visit_expr(
    pass: &DefinitionPass,
    path: Path,
    scope_expr_idx: &mut usize,
    local_var_idx: &mut usize,
    expr: &Expr
) -> SemanticValue {
    *scope_expr_idx += 1;

    let span = expr.span.clone();
    match &expr.kind {
        ExprKind::Variable(name) => {
            let value = find_or_yield(pass, path, name.clone());

            match value {
                SemanticValue::LocalVariable(type_, idx) => {
                    SemanticValue::Expr(TypedExprKind::LocalVariable(idx).expr(span, type_), Type::Never)
                }
                SemanticValue::ConstVariable(type_, idx) => {
                    SemanticValue::Expr(TypedExprKind::ConstVariable(idx).expr(span, type_), Type::Never)
                }
                value => value
            }
        }
        ExprKind::IntegerAccess(left, index) => {
            let (left, return_type) = visit_expr_expect_expr(pass, path.clone(), scope_expr_idx, local_var_idx, left);

            if let Type::Tuple(types) = &left.type_ {
                let Some(indexed_type) = types.get(*index) else {
                    error(pass, "index out of bounds on tuple", &span);
                };

                let indexed_type = indexed_type.clone();

                SemanticValue::Expr(
                    TypedExprKind::IntegerAccess(
                        Box::new(left),
                        *index
                    ).expr(span, indexed_type),
                    return_type
                )
            } else {
                error(pass, "cannot index a non-tuple", &span)
            }
        }
        ExprKind::Tuple(exprs) => {
            let mut typed_exprs = vec![];
            let mut return_type = Type::Never;

            for expr in exprs {
                let (typed_expr, sub_return_type) = visit_expr_expect_expr(pass, path.clone(), scope_expr_idx, local_var_idx, expr);
                typed_exprs.push(typed_expr);
                return_type = get_common_type(pass, &span, &return_type, &sub_return_type);
            }

            let type_ = Type::Tuple(typed_exprs.iter().map(|a| a.type_.clone()).collect());

            SemanticValue::Expr(TypedExprKind::Tuple(typed_exprs).expr(span, type_), return_type)
        }
        ExprKind::LitInt(i) => SemanticValue::Expr(
            TypedExprKind::LitInt(*i)
                .expr(span, Type::StructObject(borrow_ctx(pass).standard.int.clone())),
            Type::Never,
        ),
        ExprKind::LitUInt(i) => SemanticValue::Expr(
            TypedExprKind::LitUInt(*i)
                .expr(span, Type::StructObject(borrow_ctx(pass).standard.uint.clone())),
            Type::Never,
        ),
        ExprKind::LitFloat(i) => SemanticValue::Expr(
            TypedExprKind::LitFloat(*i)
                .expr(span, Type::StructObject(borrow_ctx(pass).standard.float.clone())),
            Type::Never,
        ),
        ExprKind::LitByte(i) => SemanticValue::Expr(
            TypedExprKind::LitByte(*i)
                .expr(span, Type::StructObject(borrow_ctx(pass).standard.byte.clone())),
            Type::Never,
        ),
        ExprKind::LitChar(i) => SemanticValue::Expr(
            TypedExprKind::LitChar(*i)
                .expr(span, Type::StructObject(borrow_ctx(pass).standard.char.clone())),
            Type::Never,
        ),
        ExprKind::LitBool(i) => SemanticValue::Expr(
            TypedExprKind::LitBool(*i)
                .expr(span, Type::StructObject(borrow_ctx(pass).standard.bool.clone())),
            Type::Never,
        ),
        ExprKind::LitUnit => unit(pass, span),
        ExprKind::LitStr(s) => {
            // let mut ctx = borrow_ctx(pass);
            // let value = ctx.new_object(Object::String(s.clone()));
            SemanticValue::Expr(
                TypedExprKind::LitStr(s.clone())
                    .expr(span, Type::StructObject(borrow_ctx(pass).standard.str.clone())),
                Type::Never,
            )
        }
        ExprKind::Return(value) => {
            let (value, return_type) = visit_expr_expect_expr(pass, path, scope_expr_idx, local_var_idx, value);

            let common_type = get_common_type(pass, &value.span, &value.type_, &return_type);

            SemanticValue::Expr(
                TypedExprKind::Return(Box::new(value)).expr(span, Type::Never),
                common_type,
            )
        }
        ExprKind::AssignVariable(left, name, right) => {
            let var_path = path.subpath(name.clone());

            let (right, return_type) = visit_expr_expect_expr(
                pass,
                path.clone(),
                scope_expr_idx,
                local_var_idx,
                right
            );

            if let Some(left) = &left {
                let left_type = visit_expr_expect_type(pass, path, left);

                if !can_cast_to(pass, &right.type_, &left_type) {
                    error(pass, "type mismatch", &span);
                }

                let idx = *local_var_idx;
                *local_var_idx += left_type.get_size();

                set(
                    pass,
                    var_path,
                    SemanticValue::LocalVariable(left_type.clone(), idx),
                );

                SemanticValue::Expr(
                    TypedExprKind::AssignVariable(Box::new(right)).expr(span, left_type),
                    return_type,
                )
            } else {
                let idx = *local_var_idx;
                *local_var_idx += right.type_.get_size();

                set(
                    pass,
                    var_path,
                    SemanticValue::LocalVariable(right.type_.clone(), idx),
                );
                SemanticValue::Expr(
                    TypedExprKind::AssignVariable(Box::new(right.clone()))
                        .expr(span, right.type_),
                    return_type,
                )
            }
        }
        ExprKind::Block(exprs, tail) => {
            let branch_path = path.subpath(format!("$branch{}", scope_expr_idx));
            let mut func_return_type = Type::Never;
            let mut statements = vec![];
            let mut scope_expr_idx = 0;
            let mut local_var_idx = *local_var_idx;

            for expr in exprs {
                let (expr, statement_return_type) = visit_expr_expect_expr(
                    pass,
                    branch_path.clone(),
                    &mut scope_expr_idx,
                    &mut local_var_idx,
                    expr,
                );
                func_return_type =
                    get_common_type(pass, &expr.span, &func_return_type, &statement_return_type);

                if expr.type_.is_never() {
                    statements.push(expr);
                    return SemanticValue::Expr(
                        TypedExprKind::Block(statements, None).expr(span, Type::Never),
                        func_return_type,
                    );
                } else {
                    statements.push(expr);
                };
            }

            if let Some(tail) = tail {
                let (tail, statement_return_type) = visit_expr_expect_expr(
                    pass,
                    path,
                    &mut scope_expr_idx,
                    &mut local_var_idx,
                    tail,
                );
                let func_return_type =
                    get_common_type(pass, &tail.span, &func_return_type, &statement_return_type);

                let tail_type = tail.type_.clone();

                SemanticValue::Expr(
                    TypedExprKind::Block(statements, Some(Box::new(tail))).expr(span, tail_type),
                    func_return_type,
                )
            } else {
                SemanticValue::Expr(
                    TypedExprKind::Block(statements, None).expr(span, unit_type(pass)),
                    func_return_type,
                )
            }
        }
        _ => todo!(),
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

fn definition(pass: &DefinitionPass, path: Path) -> Result<(), CompilerError> {
    let span = &pass.def.span;

    match &pass.def.kind {
        DefinitionKind::ConstStatement {
            binding,
            value: right,
        } => {
            let var_path = path.subpath(binding.name.clone());

            let (right, return_type) = visit_expr_expect_expr(
                pass,
                path.clone(),
                &mut 0,
                &mut 0,
                right
            );

            if let Some(left) = &binding.type_ {
                let left_type = visit_expr_expect_type(
                    pass,
                    path,
                    left
                );

                if !can_cast_to(pass, &right.type_, &left_type) {
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
            } else {
                set(pass, var_path, SemanticValue::Expr(right, return_type))
            }
        }
        DefinitionKind::StructStatement { name, fields, .. } => {
            let struct_path = path.subpath(name.clone());
            set(
                pass,
                struct_path.clone(),
                SemanticValue::Struct(struct_path.clone()),
            );

            let mut fields_processed = HashMap::new();

            for (i, Binding { type_, name, .. }) in fields.iter().enumerate() {
                if let Some(type_) = type_ {
                    let left = visit_expr_expect_type(
                        pass,
                        path.clone(),
                        type_
                    );

                    fields_processed.insert(name.clone(), (i, left));
                } else {
                    error(pass, "auto types invalid here", span);
                }
            }

            set(
                pass,
                struct_path.subpath("$fields".to_string()),
                SemanticValue::StructFields(fields_processed),
            );
        }
        DefinitionKind::FunctionStatement {
            binding,
            param_bindings,
            statement,
        } => {
            let func_path = path.subpath(binding.name.clone());

            let expected_return_type = if let Some(expr) = &binding.type_ {
                visit_expr_expect_type(pass, path.clone(), expr)
            } else {
                error(pass, "auto not allowed in function declaration", span)
            };

            let definition_idx = borrow_ctx(pass).resulting_ast.functions.len();

            set(pass, func_path.clone(), SemanticValue::Function(expected_return_type.clone(), definition_idx));

            let mut local_var_idx = 0usize;

            let mut bindings = vec![];
            for binding in param_bindings {
                let Some(type_) = &binding.type_ else {
                    error(pass, "auto not allowed in function declaration", span);
                };

                let param_type = visit_expr_expect_type(
                    pass,
                    path.clone(),
                    type_
                );
                let param_path = func_path.subpath(binding.name.clone());

                let param_idx = local_var_idx;
                local_var_idx += param_type.get_size();

                set(pass, param_path.clone(), SemanticValue::LocalVariable(param_type, param_idx));
                bindings.push(param_path);
            }

            if let Some(statement) = statement {
                let (statement, return_type) =
                    visit_expr_expect_expr(pass, func_path, &mut 0, &mut local_var_idx, statement);
                let return_type = get_common_type(pass, span, &return_type, &statement.type_);

                if !can_cast_to(pass, &return_type, &expected_return_type) {
                    error(
                        pass,
                        "expected return type does not match actual return type",
                        span,
                    );
                } else {
                    borrow_ctx(pass).resulting_ast.functions.push(statement)
                }
            } else {
                todo!("do something with abstract functions")
                // function is valid, nothing happens
            }
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
        statics: vec![],
        resulting_ast: TypedAST {
            constants: vec![],
            functions: vec![],
        },
        standard: standard_types(),
        errors: vec![],
    }));

    {
        let mut ctx = ctx.borrow_mut();
        let standard = ctx.standard.clone();

        ctx.set(standard.int.clone(), SemanticValue::Struct(standard.int));
        ctx.set(
            standard.uint.clone(),
            SemanticValue::Struct(standard.uint),
        );
        ctx.set(
            standard.float.clone(),
            SemanticValue::Struct(standard.float),
        );
        ctx.set(
            standard.char.clone(),
            SemanticValue::Struct(standard.char),
        );
        ctx.set(
            standard.byte.clone(),
            SemanticValue::Struct(standard.byte),
        );
        ctx.set(standard.str.clone(), SemanticValue::Struct(standard.str));
        ctx.set(
            standard.void.clone(),
            SemanticValue::Struct(standard.void),
        );
        ctx.set(
            standard.bool.clone(),
            SemanticValue::Struct(standard.bool),
        );
    }

    let mut definition_states = vec![];

    for def in tree.definitions {
        let ctx_clone = Rc::clone(&ctx);

        let generator = Coroutine::new(|yielder, _| {
            let err = definition(
                &DefinitionPass {
                    ctx: ctx_clone,
                    def,
                    yielder: &yielder,
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
                    if find(&mut ctx.borrow_mut(), path.clone(), name.clone()).is_some() {
                        possibly_frozen = false;
                        possibly_finished = false;
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
            break;
        }
    }

    let mut ctx = ctx.borrow_mut();

    let namespace = &ctx.namespace;

    println!("{:#?}", namespace);

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
