use std::fs::File;
use std::io::Read;
use compiler::compiler::lexer::{lex, Source};
use compiler::compiler::parser::parse;
use compiler::compiler::semantics::semantic_check;

use clap::Parser;
use compiler::common::value::{CallableObjectHeader, GCStage, ObjectHeader, Value};
use compiler::compiler::codegen::codegen;
use compiler::vm::callable::{FunctionHelper, Invoker, PuffinCallable, RuntimeError};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(value_name = "FILE")]
    file: String,

    #[arg(short='t', long)]
    log_tokens: bool,

    #[arg(short='a', long)]
    log_ast: bool,
}

fn main() {
    let args = Args::parse();

    let mut file = File::open(args.file).expect("Failed to open file");

    let mut source = String::new();

    file.read_to_string(&mut source).expect("Failed to read file");

    let source = Source::String(source);

    let lexed = lex(&source).unwrap();

    if args.log_tokens {
        println!("{:#?}", lexed);
    }

    let ast = parse(&lexed).unwrap();

    if args.log_ast {
        println!("{:#?}", ast);
    }

    let typed_ast = match semantic_check(ast) {
        Ok(typed_ast) => typed_ast,
        Err(errs) => {
            for err in errs {
                println!("semantic error: {}", source.error_message(err));
            }
            return;
        }
    };

    println!("{:#?}", typed_ast);

    let mut program = codegen(typed_ast).unwrap();

    // let garbage_collector = GarbageCollector::default();
    //
    // let print_int_func = CallableObjectHeader::new(
    //     GCStage::Static,
    //     Box::new(
    //         |_, params: Vec<Value>| {
    //             println!("{:?}", params[0]);
    //             Ok(vec![])
    //         }
    //     ) as Box<dyn PuffinCallable>
    // );

    let print_str_func = FunctionHelper::new(|_, params: Vec<Value<'_>>| {
        println!("{}", params[0].cast_string());
        Ok(vec![])
    });

    let print_int_func = FunctionHelper::new(|_, params: Vec<Value<'_>>| {
        println!("{}", params[0].cast_int());
        Ok(vec![])
    });

    let print_debug_func = FunctionHelper::new(|_, params: Vec<Value<'_>>| {
        println!("{:?}", params[0]);
        Ok(vec![])
    });

    // let print_str_func = CallableObjectHeader::new(
    //     GCStage::Static,
    //     Box::new(
    //         |_, params: Vec<Value<'_>>| {
    //             println!("{:?}", params[0]);
    //             Ok(vec![])
    //         }
    //     ) as Box<dyn PuffinCallable>
    // );

    program.link_function("package::printstr".to_string(), print_str_func).expect("linking error");
    program.link_function("package::printi".to_string(), print_int_func).expect("linking error");
    program.link_function("package::print_debug".to_string(), print_debug_func).expect("linking error");

    println!("{:#?}", program);
    
    program.execute("package::main").expect("runtime error");

    //
    // let mut linker = HashLinker::new();
    // linker.link(
    //     "print".to_string(),
    //     Value::function(&print_func as *const GCFunction)
    // );

    // let execution_result = result.execute(&linker);

    // drop(print_func)
}
