mod parser;  
use crate::parser::parser::{Parser};

fn main() {
    let parse_a = Parser::<char>::char('A').many1();

    let string = "AAAADCD";

    let result = parse_a.parse(string);

    println!("{result:?}");
}
