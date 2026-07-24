mod parser;  
use crate::parser::parser::{Parser};

fn main() {
    let parse_a = Parser::<char>::char('A');
    let parse_b = Parser::<char>::char('B');
    let parse_c = Parser::<char>::char('C');

    let parse_abc = parse_a.then(parse_b).then(parse_c);

    let string = "ABC";

    let result = parse_abc.parse(string);

    println!("{result:?}");
}
