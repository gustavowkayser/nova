mod parser;  
use crate::parser::parser::{Parser};

fn main() {
    let parse_abc = Parser::<String>::string("ABC".to_string());

    let string = "ADCD";

    let result = parse_abc.parse(string);

    println!("{result:?}");
}
