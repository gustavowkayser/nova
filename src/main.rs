mod parser;  
use crate::parser::parser::{Parser};

fn main() {
    
    let separator = Parser::<char>::char(';');
    let sep = Parser::<i64>::int().sep_by1(separator);

    let string = "Z";
    let result = sep.parse(string);

    println!("{result:?}");
}
