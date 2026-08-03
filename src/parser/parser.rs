use std::{collections::LinkedList, rc::Rc};

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

    pub fn string(expected: String) -> Parser<String>
    {
        fn char_list_to_string(
            expected_chars: impl IntoIterator<Item = char>
        ) -> String {
            expected_chars.into_iter().collect()
        }

        let list_chars: Vec<char> = expected.chars().collect();
        let list_pchars = list_chars.iter().map(|expected: &char| { Parser::<char>::char(*expected) } );
        let sequence = Parser::<char>::sequence(list_pchars);
        let parser_string = sequence.map(char_list_to_string);

        return parser_string;
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

    pub fn returnp<U>(value: U) -> Parser<U>
    where
        U: Clone + 'static
    {
        Parser::new(move |input| {
            Ok((value.clone(), input))
        })
    }

    pub fn lift2<A, B, C, F>(
        func: F,
        a: Parser<A>,
        b: Parser<B>,
    ) -> Parser<C>
    where
        A: 'static,
        B: 'static,
        C: 'static,
        F: Fn((A, B)) -> C + Clone + 'static,
    {
        a.then(b).map(func)
    }

    pub fn sequence<U>(parsers: impl IntoIterator<Item = Parser<U>>) -> Parser<LinkedList<U>>
    where 
        U: Clone + 'static
    {
        fn cons<U>(mut tuple: (U, LinkedList<U>)) -> LinkedList<U> {
            tuple.1.push_front(tuple.0);
            tuple.1
        }

        let parsers_iter: Vec<Parser<U>> = parsers.into_iter().collect();

        match parsers_iter.first() {
            Some(head) => {
                let tail = parsers_iter[1..].iter().cloned().collect::<Vec<_>>();
                Parser::<U>::lift2(cons, head.clone(), Parser::<U>::sequence(tail))
            }
            None => Parser::<U>::returnp(LinkedList::new()),
        }
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
