use std::fs::File;
use std::io::Read;
use compiler::compiler::lexer::{lex, Source};
use compiler::compiler::parser::parse;
use compiler::compiler::semantics::semantic_check;

use clap::Parser;

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

    println!("{:#?}", typed_ast)
}
