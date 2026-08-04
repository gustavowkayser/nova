mod parser;  
use crate::parser::parser::{Parser};

fn main() {
    let parse_digit = Parser::<i64>::int();
    let parse_semicolon = Parser::<char>::char(';');

    let parse_digit_then_semi = parse_digit.then(Parser::<char>::opt(parse_semicolon));

    let string = "-14354;";

    let result = parse_digit_then_semi.parse(string);

    println!("{result:?}");
}
