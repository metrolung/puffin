use std::cell::UnsafeCell;
use std::collections::HashMap;
use std::fmt::{Debug, Display, Formatter, Write};
use crate::common::value::{CallableObjectHeader, ObjectFlag, ObjectHeader, StringObjectHeader};
use crate::compiler::error::CompilerError;
use crate::compiler::parser::{BinOpKind, UnOpKind};
use crate::compiler::position::SpanPosition;
use crate::compiler::semantics::{Type, TypedAST, TypedBinOpKind, TypedUnOpKind, TypedValueExpr, TypedValueExprKind};
use crate::vm::callable::{PuffinFunction, PuffinProgram};

#[derive(Copy, Clone)]
pub enum Instruction {
    // LoadInt(isize),
    // LoadUInt(usize),
    // LoadVoid,
    // LoadFloat(fsize),
    // LoadByte(u8),
    // LoadChar(char),
    // LoadBool(bool),
    Load(u64),
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

    AddI,
    AddF,

    SubI,
    SubF,

    MulI,
    MulF,

    DivI,
    DivU,
    DivF,

    NegI,
    NegF,
    Not,

    Eq(u32), // operand size
    Ne(u32), // operand size
    LtI,
    LtU,
    LtF,
    LeI,
    LeU,
    LeF,
    GtI,
    GtU,
    GtF,
    GeI,
    GeU,
    GeF,
}

impl Debug for Instruction {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Instruction::Load(a) =>                         f.write_fmt(format_args!("load {}", a)),
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
            Instruction::AddI =>                            f.write_str("addi"),
            Instruction::AddF =>                            f.write_str("addf"),
            Instruction::SubI =>                            f.write_str("subi"),
            Instruction::SubF =>                            f.write_str("subf"),
            Instruction::MulI =>                            f.write_str("muli"),
            Instruction::MulF =>                            f.write_str("mulf"),
            Instruction::DivI =>                            f.write_str("divi"),
            Instruction::DivU =>                            f.write_str("divu"),
            Instruction::DivF =>                            f.write_str("divf"),
            Instruction::NegI =>                             f.write_str("negi"),
            Instruction::NegF =>                             f.write_str("negf"),
            Instruction::Not =>                             f.write_str("not"),
            Instruction::Eq(a) =>                           f.write_fmt(format_args!("eq {}", a)),
            Instruction::Ne(a) =>                           f.write_fmt(format_args!("ne {}", a)),
            Instruction::LtI =>                              f.write_str("lti"),
            Instruction::LtU =>                              f.write_str("ltu"),
            Instruction::LtF =>                              f.write_str("ltf"),
            Instruction::LeI =>                              f.write_str("lei"),
            Instruction::LeU =>                              f.write_str("leu"),
            Instruction::LeF =>                              f.write_str("lef"),
            Instruction::GtI =>                              f.write_str("gti"),
            Instruction::GtU =>                              f.write_str("gtu"),
            Instruction::GtF =>                              f.write_str("gtf"),
            Instruction::GeI =>                              f.write_str("gei"),
            Instruction::GeU =>                              f.write_str("geu"),
            Instruction::GeF =>                              f.write_str("gef"),
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
        TypedValueExprKind::Primitive(i) => {
            ctx.instruct(Instruction::Load(i));
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
                ctx.string_table.push(ObjectHeader::new_static(s));
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
        TypedValueExprKind::BinOp(op_kind, left, right) => {
            let size = left.type_.get_size();
            visit_expr(ctx, *left)?;
            visit_expr(ctx, *right)?;
            ctx.instruct(match op_kind {
                TypedBinOpKind::AddI => Instruction::AddI,
                TypedBinOpKind::AddF => Instruction::AddF,
                TypedBinOpKind::SubI => Instruction::SubI,
                TypedBinOpKind::SubF => Instruction::SubF,
                TypedBinOpKind::MulI => Instruction::MulI,
                TypedBinOpKind::MulF => Instruction::MulF,
                TypedBinOpKind::DivI => Instruction::DivI,
                TypedBinOpKind::DivU => Instruction::DivU,
                TypedBinOpKind::DivF => Instruction::DivF,
                TypedBinOpKind::Eq => Instruction::Eq(size),
                TypedBinOpKind::Ne => Instruction::Ne(size),
                TypedBinOpKind::LtI => Instruction::LtI,
                TypedBinOpKind::LtU => Instruction::LtU,
                TypedBinOpKind::LtF => Instruction::LtF,
                TypedBinOpKind::LeI => Instruction::LeI,
                TypedBinOpKind::LeU => Instruction::LeU,
                TypedBinOpKind::LeF => Instruction::LeF,
                TypedBinOpKind::GtI => Instruction::GtI,
                TypedBinOpKind::GtU => Instruction::GtU,
                TypedBinOpKind::GtF => Instruction::GtF,
                TypedBinOpKind::GeI => Instruction::GeI,
                TypedBinOpKind::GeU => Instruction::GeU,
                TypedBinOpKind::GeF => Instruction::GeF,
            })
        },
        TypedValueExprKind::UnOp(op_kind, value) => {
            visit_expr(ctx, *value)?;
            ctx.instruct(match op_kind {
                TypedUnOpKind::NegI => Instruction::NegI,
                TypedUnOpKind::NegF => Instruction::NegF,
                TypedUnOpKind::Not => Instruction::Not,
            })
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

        codegen_ctx.function_table.push(ObjectHeader::new(Box::new(PuffinFunction {
            instructions
        })));
    }

    Ok(PuffinProgram {
        string_table: codegen_ctx.string_table,
        function_table: codegen_ctx.function_table,
        function_lookup: codegen_ctx.function_lookup,
    })
}