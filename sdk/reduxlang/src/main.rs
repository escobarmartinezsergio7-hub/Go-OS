use std::io::{self, Write};

use reduxlang::eval::Env;
use reduxlang::lexer::Lexer;
use reduxlang::parser::Parser;

fn main() {
    println!("ReduxLang REPL (type :quit to exit, :vars to inspect env)");

    let mut env = Env::default();

    loop {
        print!("> ");
        io::stdout().flush().expect("flush");

        let mut line = String::new();
        if io::stdin().read_line(&mut line).is_err() {
            break;
        }

        let input = line.trim();
        if input.is_empty() {
            continue;
        }
        if input == ":quit" {
            break;
        }
        if input == ":vars" {
            for (k, v) in env.dump() {
                println!("{k} = {v}");
            }
            continue;
        }

        let lexer = Lexer::new(input);
        let mut parser = Parser::new(lexer);

        match parser.parse_program() {
            Ok(program) => match env.eval_program(&program) {
                Ok(Some(v)) => println!("{v}"),
                Ok(None) => {}
                Err(e) => eprintln!("eval error: {e}"),
            },
            Err(e) => eprintln!("parse error: {e}"),
        }
    }
}
