use std::rc::Rc;

pub type ParseResult<'a, T> = Result<(T, &'a str), String>;

pub struct Parser<T> {
    parser: Rc<dyn for<'a> Fn(&'a str) -> ParseResult<'a, T>>,
}

impl<T> Clone for Parser<T> {
    fn clone(&self) -> Self {
        Self {
            parser: self.parser.clone(),
        }
    }
}

impl<T> Parser<T> {
    pub fn new<F>(f: F) -> Self
    where
        F: for<'a> Fn(&'a str) -> ParseResult<'a, T> + 'static,
    {
        Self {
            parser: Rc::new(f),
        }
    }

    pub fn parse<'a>(&self, input: &'a str) -> ParseResult<'a, T> {
        (self.parser)(input)
    }

    pub fn char(expected: char) -> Parser<char> 
    {
        Parser::new(move |input| {
            match input.chars().next() {
                Some(found) if found == expected => {
                    Ok((found, &input[found.len_utf8()..]))
                }
                Some(found) => Err(format!(
                    "Expected '{}', found '{}'",
                    expected,
                    found
                )),
                None => Err(format!("Expected '{}', found EOF", expected)),
            }
        })
    }

    pub fn any(expected_chars: impl IntoIterator<Item = char>) -> Parser<char>
    {
        let chars: Vec<char> = expected_chars.into_iter().collect();

        Parser::new(move |input| {
            match input.chars().next() {
                Some(c) if chars.contains(&c) => {
                    Ok((c, &input[c.len_utf8()..]))
                }
                Some(c) => Err(format!("Unexpected '{}'", c)),
                None => Err("Unexpected EOF".into()),
            }
        })
    }

    pub fn choice<U>(parsers: impl IntoIterator<Item = Parser<U>>) -> Parser<U>
    where
        U: 'static
    {
        let parsers_col: Vec<Parser<U>> = parsers.into_iter().collect();

        Parser::new(move |input| {
            for parser in &parsers_col {
                let result = parser.parse(input);
                if result.is_ok() { return result; }
            }

            return Err(format!("Unexpected input {input:?}"));
        })
    }

    pub fn then<U>(self, other: Parser<U>) -> Parser<(T, U)>
    where
        T: 'static,
        U: 'static,
    {
        Parser::new(move |input| {
            let (left, input) = self.parse(input)?;
            let (right, input) = other.parse(input)?;

            Ok(((left, right), input))
        })
    }

    pub fn or(self, other: Parser<T>) -> Parser<T>
    where
        T: 'static,
    {
        Parser::new(move |input| {
            self.parse(input).or_else(|_| other.parse(input))
        })
    }

    pub fn map<U, F>(self, mapper: F) -> Parser<U>
    where
        T: 'static,
        U: 'static,
        F: Fn(T) -> U + 'static,
    {
        Parser::new(move |input| {
            let (value, input) = self.parse(input)?;
            Ok((mapper(value), input))
        })
    }
}
