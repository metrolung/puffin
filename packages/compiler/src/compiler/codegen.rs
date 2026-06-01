use std::cell::UnsafeCell;
use std::collections::HashMap;
use std::fmt::{Debug, Display, Formatter, Write};
use std::pin::Pin;
use std::rc::Rc;
use crate::common::fsize::fsize;
use crate::common::value::{CallableObjectHeader, GCStage, ObjectHeader, StringObjectHeader, Value};
use crate::compiler::error::CompilerError;
use crate::compiler::parser::{BinOpKind, UnOpKind};
use crate::compiler::position::SpanPosition;
use crate::compiler::semantics::{Type, TypedAST, TypedValueExpr, TypedValueExprKind};
use crate::vm::callable::{PuffinFunction, PuffinProgram};

#[derive(Copy, Clone)]
pub enum Instruction {
    LoadInt(isize),
    LoadUInt(usize),
    LoadVoid,
    LoadFloat(fsize),
    LoadByte(u8),
    LoadChar(char),
    LoadBool(bool),
    LoadString(usize), // ref -> static string
    LoadObject(usize), // ref -> static object
    LoadFunction(usize), // ref -> static function
    LoadLocal(u32, u32), // local_idx, size

    CopyLocal(u32, u32, u32), // src, dest, size
    CopyLocalAndReposition(u32, u32, u32), // src, dest, size
    Reposition(u32), // local_idx

    Invoke(u32, u32), // callback_local_idx, input_size
    Return(u32), // return_idx, return_size
    Unimplemented,
    Test,
    Jump(usize),

    Add,
    Sub,
    Mul,
    Div,
    Neg,
    Not,

    Eq(u32), // operand size
    Ne(u32), // operand size
    Lt,
    Le,
    Gt,
    Ge,
}

impl Debug for Instruction {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Instruction::LoadInt(a) =>                      f.write_fmt(format_args!("loadint {}", a)),
            Instruction::LoadUInt(a) =>                     f.write_fmt(format_args!("loaduint {}", a)),
            Instruction::LoadFloat(a) =>                    f.write_fmt(format_args!("loadfloat {}", a)),
            Instruction::LoadByte(a) =>                     f.write_fmt(format_args!("loadbyte {}", a)),
            Instruction::LoadChar(a) =>                     f.write_fmt(format_args!("loadchar {}", a)),
            Instruction::LoadBool(a) =>                     f.write_fmt(format_args!("loadbool {}", a)),
            Instruction::LoadString(a) =>                   f.write_fmt(format_args!("loadstring {}", a)),
            Instruction::LoadObject(a) =>                   f.write_fmt(format_args!("loadobject {}", a)),
            Instruction::LoadFunction(a) =>                 f.write_fmt(format_args!("loadfunction {}", a)),
            Instruction::LoadLocal(a, b) =>                 f.write_fmt(format_args!("loadlocal {} {}", a, b)),
            Instruction::CopyLocal(a, b, c) =>              f.write_fmt(format_args!("copylocal {} {} {}", a, b, c)),
            Instruction::CopyLocalAndReposition(a, b, c) => f.write_fmt(format_args!("copyrepos {} {} {}", a, b, c)),
            Instruction::Reposition(a) =>                   f.write_fmt(format_args!("repos {}", a)),
            Instruction::Invoke(a, b) =>                    f.write_fmt(format_args!("invoke {} {}", a, b)),
            Instruction::Return(a) =>                       f.write_fmt(format_args!("return {}", a)),
            Instruction::Unimplemented =>                   f.write_str("unimplemented"),
            Instruction::Test =>                            f.write_str("test"),
            Instruction::Jump(a) =>                         f.write_fmt(format_args!("jump {}", a)),
            Instruction::LoadVoid =>                        f.write_str("loadunit"),
            Instruction::Add =>                             f.write_str("add"),
            Instruction::Sub =>                             f.write_str("sub"),
            Instruction::Mul =>                             f.write_str("mul"),
            Instruction::Div =>                             f.write_str("div"),
            Instruction::Neg =>                             f.write_str("neg"),
            Instruction::Not =>                             f.write_str("not"),
            Instruction::Eq(a) =>                           f.write_fmt(format_args!("eq {}", a)),
            Instruction::Ne(a) =>                           f.write_fmt(format_args!("ne {}", a)),
            Instruction::Lt =>                              f.write_str("lt"),
            Instruction::Le =>                              f.write_str("le"),
            Instruction::Gt =>                              f.write_str("gt"),
            Instruction::Ge =>                              f.write_str("ge"),
        }
    }
}

// impl CodegenContext {
//     // fn instruct(&mut self, instruction: Instruction) {
//     //     self.incomplete_instructions.push(IncompleteInstruction::Instruction(instruction));
//     // }
//
//     // fn partial_instruct(&mut self, instruction: IncompleteInstruction) {
//     //     self.incomplete_instructions.push(instruction);
//     // }
// }

// pub struct Program {
//     pub statics: Vec<GarbageCollected>,
//     pub instructions: Vec<Instruction>
// }

fn visit_expr(ctx: &mut FunctionGenContext, expr: TypedValueExpr) -> Result<(), CompilerError> {
    let span = expr.span;
    let expr_type = expr.type_;
    let place = expr.place;

    match expr.kind {
        TypedValueExprKind::LitInt(i) => {
            ctx.instruct(Instruction::LoadInt(i));
        }
        TypedValueExprKind::LitUInt(i) => {
            ctx.instruct(Instruction::LoadUInt(i));
        }
        TypedValueExprKind::LitVoid => {
            ctx.instruct(Instruction::LoadVoid);
        }
        TypedValueExprKind::LitFloat(f) => {
            ctx.instruct(Instruction::LoadFloat(f));
        }
        TypedValueExprKind::LitBool(b) => {
            ctx.instruct(Instruction::LoadBool(b));
        }
        TypedValueExprKind::LitChar(c) => {
            ctx.instruct(Instruction::LoadChar(c));
        }
        TypedValueExprKind::LitByte(i) => {
            ctx.instruct(Instruction::LoadByte(i));
        }
        TypedValueExprKind::GetFunction(func_idx) => {
            ctx.instruct(Instruction::LoadFunction(func_idx));
        }
        TypedValueExprKind::LitStr(s) => {
            let static_idx = if ctx.string_lookup.contains_key(&s) {
                let static_idx = ctx.string_lookup[&s];
                static_idx
            } else {
                let string_idx = ctx.string_table.len();
                ctx.string_table.push(ObjectHeader::new(GCStage::Static, s));
                string_idx
            };
            ctx.instruct(Instruction::LoadString(static_idx));
        }
        TypedValueExprKind::LocalAccess(local_idx) => {
            ctx.instruct(Instruction::LoadLocal(local_idx, expr_type.get_size()));
        }
    TypedValueExprKind::Block(statements, tail) => {
            for statement in statements {
                visit_expr(ctx, statement)?;
            }

            if let Some(tail) = tail {
                let tail_size = tail.type_.get_size();
                let tail_place = tail.place;
                visit_expr(ctx, *tail)?;

                ctx.instruct(Instruction::CopyLocalAndReposition(tail_place, place, tail_size));
            } else {
                ctx.instruct(Instruction::Reposition(place));
            }
        }
        TypedValueExprKind::Return(return_val) => {
            if let Some(return_val) = return_val {
                let return_size = return_val.type_.get_size();
                let return_val_place = return_val.place;
                visit_expr(ctx, *return_val)?;
                ctx.instruct(Instruction::CopyLocal(return_val_place, 0, return_size));
                ctx.instruct(Instruction::Return(return_size))
            } else {
                ctx.instruct(Instruction::Return(0))
            }
        }
        TypedValueExprKind::Invoke(callable, params) => {
            let callable_place = callable.place;

            let (param_types, return_type) = match &callable.type_ {
                Type::Callable(a, b) => (a, b),
                _ => panic!("invoking with a non callable on the left")
            };

            let param_size = param_types.iter().map(|p| p.get_size()).sum();
            let return_size = return_type.get_size();

            visit_expr(ctx, *callable)?;
            for expr in params {
                visit_expr(ctx, expr)?;
            }
            ctx.instruct(Instruction::Invoke(callable_place, param_size));
            ctx.instruct(Instruction::CopyLocalAndReposition(place+1, place, return_size))
        },
        TypedValueExprKind::Tuple(exprs) => {
            for expr in exprs {
                visit_expr(ctx, expr)?;
            }
        }
        TypedValueExprKind::ObjectAccess(_, _) => todo!(),
        TypedValueExprKind::TupleAccess(tuple_expr, offset) => {
            let tuple_place = tuple_expr.place;
            visit_expr(ctx, *tuple_expr)?;
            ctx.instruct(Instruction::CopyLocalAndReposition(tuple_place + offset, tuple_place, expr_type.get_size()));
        }
        TypedValueExprKind::ConstVariable(idx) => {
            visit_expr(ctx, ctx.constants[idx].clone())?;
        },
        TypedValueExprKind::If(cond, main_branch, else_branch) => {
            visit_expr(ctx, *cond)?;
            ctx.instruct(Instruction::Test);

            let on_failure = ctx.instruct_later();

            visit_expr(ctx, *main_branch)?;

            if let Some(else_branch) = else_branch {
                let end_of_main = ctx.instruct_later();
                let after_main = ctx.next_instruction_idx();
                visit_expr(ctx, *else_branch)?;
                let after_else = ctx.next_instruction_idx();
                ctx.instruct_at(end_of_main, Instruction::Jump(after_else));
                ctx.instruct_at(on_failure, Instruction::Jump(after_main));
            } else {
                let after_main = ctx.next_instruction_idx();
                ctx.instruct_at(on_failure, Instruction::Jump(after_main));
            }
        },
        TypedValueExprKind::NoOpCast(value) => {
            visit_expr(ctx, *value)?;
        }
        TypedValueExprKind::ReassignVariable(_, _) => todo!(),
        TypedValueExprKind::BinOp(binop_kind, left, right) => {
            let size = left.type_.get_size();
            visit_expr(ctx, *left)?;
            visit_expr(ctx, *right)?;
            match binop_kind {
                BinOpKind::Add => ctx.instruct(Instruction::Add),
                BinOpKind::Sub => ctx.instruct(Instruction::Sub),
                BinOpKind::Mul => ctx.instruct(Instruction::Mul),
                BinOpKind::Div => ctx.instruct(Instruction::Div),
                BinOpKind::Eq => ctx.instruct(Instruction::Eq(size)),
                BinOpKind::Ne => ctx.instruct(Instruction::Ne(size)),
                BinOpKind::Lt => ctx.instruct(Instruction::Lt),
                BinOpKind::Le => ctx.instruct(Instruction::Le),
                BinOpKind::Gt => ctx.instruct(Instruction::Gt),
                BinOpKind::Ge => ctx.instruct(Instruction::Ge),
            }
        },
        TypedValueExprKind::UnOp(unop_kind, value) => {
            visit_expr(ctx, *value)?;
            match unop_kind {
                UnOpKind::Pos => (),
                UnOpKind::Neg => ctx.instruct(Instruction::Neg),
                UnOpKind::Not => ctx.instruct(Instruction::Not),
            }
        }
    }
    Ok(())
}


struct CodegenContext {
    string_table: Vec<StringObjectHeader>,
    function_table: Vec<CallableObjectHeader>,
    string_lookup: HashMap<String, usize>,
    function_lookup: HashMap<String, usize>,

    constants: Vec<TypedValueExpr>,
    // static_pool: StaticPool
    // incomplete_instructions: Vec<IncompleteInstruction>,
    // function_idx_to_instruction_idx: Vec<usize>,
}

struct FunctionGenContext<'a> {
    string_table: &'a mut Vec<StringObjectHeader>,
    function_table: &'a Vec<CallableObjectHeader>,
    string_lookup: &'a mut HashMap<String, usize>,
    function_lookup: &'a HashMap<String, usize>,

    constants: &'a Vec<TypedValueExpr>,
    // static_pool: &'a mut StaticPool,
    // string_dedup: &'a mut HashMap<String, usize>,
    instructions: Vec<Instruction>
}

impl FunctionGenContext<'_> {
    fn instruct(&mut self, instruction: Instruction) {
        self.instructions.push(instruction)
    }

    fn instruct_later(&mut self) -> usize {
        let idx = self.instructions.len();
        self.instructions.push(Instruction::Unimplemented);
        idx
    }

    fn instruct_at(&mut self, idx: usize, instruction: Instruction) {
        self.instructions[idx] = instruction;
    }

    fn next_instruction_idx(&self) -> usize {
        self.instructions.len()
    }
}

pub fn codegen(tree: TypedAST) -> Result<PuffinProgram, CompilerError> {
    let mut codegen_ctx = CodegenContext {
        string_table: vec![],
        function_table: vec![],
        string_lookup: HashMap::new(),
        function_lookup: HashMap::new(),
        constants: tree.constants,
    };

    for function in tree.functions {
        let mut function_ctx = FunctionGenContext {
            string_table: &mut codegen_ctx.string_table,
            function_table: &mut codegen_ctx.function_table,
            string_lookup: &mut codegen_ctx.string_lookup,
            function_lookup: &mut codegen_ctx.function_lookup,

            constants: &codegen_ctx.constants,
            instructions: vec![]
        };

        if let Some(body) = function.body {
            let return_size = body.type_.get_size();
            let body_place = body.place;
            visit_expr(&mut function_ctx, body)?;
            function_ctx.instruct(Instruction::CopyLocal(body_place, 0, return_size));
            function_ctx.instruct(Instruction::Return(return_size));
        } else {
            function_ctx.instruct(Instruction::Unimplemented);
        }

        let instructions = function_ctx.instructions;

        if let Some(exported_name) = &function.export_name {
            let index = codegen_ctx.function_table.len();
            codegen_ctx.function_lookup.insert(exported_name.clone(), index);
        }

        codegen_ctx.function_table.push(ObjectHeader::new(GCStage::Static, Box::new(PuffinFunction {
            instructions
        })));
    }

    Ok(PuffinProgram {
        string_table: codegen_ctx.string_table,
        function_table: codegen_ctx.function_table,
        function_lookup: codegen_ctx.function_lookup,
    })
}