mod lexer;  

fn main() {
    let lexer = lexer::lexer::Lexer::new();
    let tokens = lexer.tokenize("test/api.nova").unwrap();

    for token in tokens {
        println!("{:?}({})", token.token_type, token.value.as_ref().unwrap_or(&"".into()));
    }
}
