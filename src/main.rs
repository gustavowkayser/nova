mod parser;  
use crate::parser::parser::{Parser, any_of, parse_char};

fn main() {
    let parse_a = parse_char('A');
    let parse_b = parse_char('B');
    let parse_c = parse_char('C');

    let parse_a_b = any_of(['A', 'B'].to_vec());

    let string = "ZCABC";
    let parse_ab = parse!(parse_a <|> parse_b <|> parse_c);

    let string = "ABC";
    let string2 = "ZCA";

    let result = parse_a_b.parse(string);
    let result2 = parse_a_b.parse(string2);

    println!("{result:?}");
    println!("{result2:?}");

    let result = parse_ab.parse(string);

    println!("{result:?}");
}
