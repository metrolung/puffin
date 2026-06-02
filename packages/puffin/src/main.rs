use std::fs::{read_link, File};
use std::io::{stdin, Read};
use std::str::FromStr;
use compiler::compiler::lexer::{lex, Source};
use compiler::compiler::parser::parse;
use compiler::compiler::semantics::semantic_check;

use clap::Parser;
use compiler::common::raw_value::{RawValuePrimitive, Value, ValueArray};
use compiler::common::value::ObjectHeader;
use compiler::compiler::codegen::codegen;
use compiler::vm::callable::{FunctionHelper};
use compiler::vm::garbage_collector::Heap;

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

    program.link_function("package::printi".to_string(), FunctionHelper::new(|_, params| {
        println!("{}", params[0].primitive().int());
        Ok(vec![].into())
    })).expect("linking error");

    program.link_function("package::println".to_string(), FunctionHelper::new(|_, params| {
        println!("{}", unsafe { &params[0].reference().unwrap().string.value });
        Ok(vec![].into())
    })).expect("linking error");

    program.link_function("package::streq".to_string(), FunctionHelper::new(|_, params| {
        let string0 = unsafe { &params[0].reference().unwrap().string.value };
        let string1 = unsafe { &params[1].reference().unwrap().string.value };
        Ok(vec![
            Value::from((string0 == string1).into()).strong(),
        ].into())
    })).expect("linking error");

    program.link_function("package::readln".to_string(), FunctionHelper::new(|ctx, params| {
        let mut s = String::new();
        stdin().read_line(&mut s).expect("reading stdin");
        s.remove(s.len() - 1);

        let s = ctx.runtime.heap.new_string(ObjectHeader::new(s));

        Ok(vec![
            s.strong()
        ].into())
    })).expect("linking error");

    program.link_function("package::readi".to_string(), FunctionHelper::new(|ctx, params| {
        let mut s = String::new();
        stdin().read_line(&mut s).expect("reading stdin");
        s.remove(s.len() - 1);

        let i = Value::from(i64::from_str(&s).expect("could not read int").into());

        Ok(vec![
            i.strong()
        ].into())
    })).expect("linking error");

    program.link_function("package::concat".to_string(), FunctionHelper::new(|ctx, params| {
        let string0 = unsafe { &params[0].reference().unwrap().string.value };
        let string1 = unsafe { &params[1].reference().unwrap().string.value };

        let result = ctx.runtime.heap.new_string(ObjectHeader::new(format!("{}{}", string0, string1).to_string())).strong();

        Ok(vec![
            result
        ].into())
    })).expect("linking error");

    println!("{:#?}", program);

    let mut heap = Heap::new();

    program.execute(&mut heap, "package::main").expect("runtime error");

    drop(heap);
}
