use std::collections::HashMap;
use crate::common::fsize::uhalf;
use crate::common::value::{GcStage, Object};
use crate::compiler::error::CompilerError;
use crate::compiler::semantics::{TypedAST, TypedExpr, TypedExprKind};






pub enum Instruction {
    LoadInt(isize),
    LoadStatic(usize),
    Call(uhalf, usize), // input_size, function_location
    Return(uhalf), // return_size
}

enum IncompleteInstruction {
    Instruction(Instruction),
    Call(uhalf, usize), // input_size, func_idx
}


struct CodegenContext {
    string_dedup: HashMap<String, usize>,
    statics: Vec<Object>,
    constants: Vec<TypedExpr>,
    incomplete_instructions: Vec<IncompleteInstruction>,
    function_idx_to_instruction_idx: Vec<usize>,
}

impl CodegenContext {
    fn instruct(&mut self, instruction: Instruction) {
        self.incomplete_instructions.push(IncompleteInstruction::Instruction(instruction));
    }

    fn partial_instruct(&mut self, instruction: IncompleteInstruction) {
        self.incomplete_instructions.push(instruction);
    }
}

pub struct Program {
    pub statics: Vec<Object>,
    pub instructions: Vec<Instruction>
}

fn visit_node(ctx: &mut CodegenContext, expr: TypedExpr) -> Result<(), CompilerError> {
    let span = expr.span;
    let expr_type = expr.type_;

    match expr.kind {
        TypedExprKind::LitInt(i) => {
            ctx.instruct(Instruction::LoadInt(i))
        }
        TypedExprKind::LitStr(s) => {
            let static_idx = if ctx.string_dedup.contains_key(&s) {
                let static_idx = ctx.string_dedup[&s];
                static_idx
            } else {
                let static_idx = ctx.statics.len();
                ctx.statics.push(Object::string(s, GcStage::Static));
                static_idx
            };
            ctx.instruct(Instruction::LoadStatic(static_idx))
        }
        _ => todo!(),
    }

    Ok(())
}

pub fn codegen(tree: TypedAST) -> Result<Program, CompilerError> {
    let mut ctx = CodegenContext {
        string_dedup: HashMap::new(),
        statics: vec![],
        constants: tree.constants,
        incomplete_instructions: vec![],
        function_idx_to_instruction_idx: vec![],
    };

    for function_expr in tree.functions {
        let idx = ctx.incomplete_instructions.len();
        ctx.function_idx_to_instruction_idx.push(idx);
        visit_node(&mut ctx, function_expr)?;
    }

    let mut complete_instructions = vec![];
    for instruction in ctx.incomplete_instructions {
        match instruction {
            IncompleteInstruction::Instruction(p0) => {
                complete_instructions.push(p0);
            }
            IncompleteInstruction::Call(p0, p1) => {
                complete_instructions.push(Instruction::Call(p0, ctx.function_idx_to_instruction_idx[p1]))
            }
        }
    }

    Ok(Program {
        instructions: complete_instructions,
        statics: ctx.statics,
    })
}