mod parser;  

fn main() {
    let parser = parser::parser::Parser::new();
    let ast = parser.parse("test/api.nova").unwrap();

    println!("AST: {:?}", ast);
}
