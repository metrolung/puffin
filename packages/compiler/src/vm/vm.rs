// use crate::common::value::Value;
// use crate::compiler::codegen::{Instruction, Program};
//
// struct StackFrame {
//     return_adr: usize,
//     frame_start: usize,
// }
//
// struct VMContext {
//     program_ptr: usize,
//     stack: Vec<Value>,
//     stack_assoc: Vec<StackFrame>,
//     end_program: bool,
// }
//
//
//
//
// fn do_instruction(ctx: &mut VMContext, instruction: Instruction) {
//     match instruction {
//         Instruction::LoadInt(i) =>
//             ctx.stack.push(i.into()),
//         Instruction::LoadStatic(i) => {
//             let obj = ctx.program.statics.get_mut(i).unwrap();
//             ctx.stack.push(obj.into())
//         }
//         Instruction::Copy(local_idx, size) => {
//             let frame_start = ctx.stack_assoc.last().unwrap().frame_start;
//             let stack_idx_start = frame_start + local_idx as usize;
//             let stack_idx_end = stack_idx_start + size as usize;
//
//             for stack_idx in stack_idx_start..stack_idx_end {
//                 let value = ctx.stack.get(stack_idx).unwrap();
//                 ctx.stack.push(*value)
//             }
//         }
//         Instruction::Call(_, _) => {}
//         Instruction::Return(return_size) => {
//             if ctx.stack_assoc.len() == 1 {
//                 ctx.end_program = true;
//             }
//
//         }
//         Instruction::Quit =>
//             ctx.end_program = true,
//     }
// }
//
//
// pub fn interpret(program: Program) {
//     let mut ctx = VMContext {
//         program,
//         program_ptr: 0,
//         stack: vec![],
//         return_stack: vec![],
//         end_program: false,
//     };
//
//     loop {
//         if !ctx.end_program {
//             break;
//         }
//
//         let program_ptr = ctx.program_ptr;
//         let instruction = *ctx.program.instructions.get(program_ptr).unwrap();
//
//         do_instruction(&mut ctx, instruction);
//     }
// }