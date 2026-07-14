mod parser;  
use crate::parser::parser::{Parser, parse_char};

fn main() {
    let parse_a = parse_char('A');
    let parse_b = parse_char('B');
    let parse_c = parse_char('C');

    let string = "ZCABC";
    let parse_ab = parse!(parse_a <|> parse_b <|> parse_c);

    let result = parse_ab.parse(string);

    println!("{result:?}");
}
