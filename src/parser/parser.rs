type ParseResult<'a, T> = Result<(T, &'a str), String>;
pub trait Parser<'a, T> {
    fn parse(&self, input: &'a str) -> ParseResult<'a, T>;
}

impl<'a, T, F> Parser<'a, T> for F
where
    F: Fn(&'a str) -> ParseResult<'a, T>,
{
    fn parse(&self, input: &'a str) -> ParseResult<'a, T> {
        self(input)
    }
}

pub fn parse_char(character: char) -> impl Parser<'static, char>
{
    let inner_fn = move |input: &'static str| -> ParseResult<char> {
        let input_char = input.chars().next();
        match input_char {
            Some(found) if found == character => Ok((character, &input[found.len_utf8()..])),
            _ => Err(format!("Expected {character}, found {:?}", input_char.expect("Expect to be char")))
        }
    };

    return inner_fn;
}

// And then
// Fn(Parser<T>, Parser<T>) -> Parser<(T, T)>

pub fn and_then<T>(
    parser_before: impl Parser<'static, T>, 
    parser_after: impl Parser<'static, T>
) -> impl Parser<'static, (T, T)> {
    
    let inner_fn = move |input: &'static str| -> ParseResult<(T, T)> {
        let result_before = parser_before.parse(input);

        if result_before.is_err() {
            return Err(result_before.err().expect("Expected to be string"));
        }

        let result_before_data = result_before.unwrap();

        let new_input = result_before_data.1;
        let result_after = parser_after.parse(new_input);

        if result_after.is_err() {
            return Err(result_after.err().expect("Expeted to be string"));
        }

        let result_after_data = result_after.unwrap();

        Ok(((result_before_data.0, result_after_data.0), result_after_data.1))
    };

    return inner_fn;
}

// Or else
// (Parser<T>, Parser<T>) -> Parser<T>

pub fn or_else<T>(
    parser_before: impl Parser<'static, T>,
    parser_after: impl Parser<'static, T>
) -> impl Parser<'static, T> {

    let inner_fn = move |input: &'static str| -> ParseResult<T> {
        let before_result = parser_before.parse(&input);

        if before_result.is_ok() {
            return before_result;
        }

        let after_result = parser_after.parse(&input);

        return after_result;
    };

    return inner_fn;
}