use std::{collections::LinkedList, error::Error, fmt, rc::Rc};

pub type ParseResult<'a, T> = Result<(T, &'a str), ParseError>;

/// A parse failure, tagged with how far into the input it happened.
///
/// The position is what lets `or` and `choice` report the most useful of
/// several failures: the branch that consumed the most input before failing
/// is almost always the one the author intended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    /// Bytes of input still unconsumed at the point of failure. Fewer bytes
    /// left means the parser got further.
    pub remaining: usize,
}

impl ParseError {
    /// Records a failure at the start of `remaining_input`.
    pub fn new(remaining_input: &str, message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            remaining: remaining_input.len(),
        }
    }

    /// Keeps whichever of two failures got further through the input,
    /// preferring `self` when they tie.
    pub fn best(self, other: Self) -> Self {
        if other.remaining < self.remaining { other } else { self }
    }

    /// Byte offset of the failure within `input`, for reporting a location.
    /// `input` must be the whole text originally handed to the parser.
    pub fn position(&self, input: &str) -> usize {
        input.len().saturating_sub(self.remaining)
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.message)
    }
}

impl Error for ParseError {}

/// Names whatever comes next in `input`, for use in error messages.
fn describe_next(input: &str) -> String {
    match input.chars().next() {
        Some(found) => format!("'{}'", found),
        None => "EOF".to_string(),
    }
}

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
        Parser::<char>::satisfy(
            move |found| found == expected,
            format!("'{}'", expected),
        )
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

    pub fn int() -> Parser<i64>
    {
        fn result_to_int(args: (Option<char>, Vec<char>)) -> i64 {
            let string: String = args.1.into_iter().collect();
            let mut num: i64 = string.parse().unwrap();

            match args.0 {
                Some(_char) => { num = -num; }
                None => { }
            }

            return num;
        }

        let digit = Parser::<char>::any("1234567890".chars());

        let digits = digit.many1();
        let sign_parser = Parser::<char>::opt(Parser::<char>::char('-'));
        
        return sign_parser.then(digits).map(result_to_int);
    }

    pub fn opt<A>(parser: Parser<A>) -> Parser<Option<A>>
    where 
        A: Clone + 'static,
    {
        let some = parser.map(Some);
        let none = Parser::<A>::returnp(None);
        return some.or(none);
    }

    pub fn many(self) -> Parser<Vec<T>>
    where 
        T: 'static
    {
        return Parser::<Vec<T>>::new(move |input| {
            Ok(Parser::<T>::parse_zero_or_more(self.clone(), input))
        });
    }

    pub fn many1(self) -> Parser<Vec<T>>
    where 
        T: 'static
    {
        return Parser::<Vec<T>>::new(move |input| {
            let first_result = self.parse(input);

            match first_result {
                Ok(success) => {
                    let first_value = success.0;
                    let after_first = success.1;

                    let (mut subsequent_values, remaining) = 
                    Parser::<T>::parse_zero_or_more(self.clone(), after_first);

                    subsequent_values.insert(0, first_value);

                    Ok((subsequent_values, remaining))
                }
                Err(error) => {
                    Err(error)
                }
            }
        });
    }

    pub fn ignore_left<A>(self: Parser<T>, other: Parser<A>) -> Parser<A>
    where
        A: 'static,
        T: 'static,
    {
        let both = self.then(other);
        return both.map( |(_a, b)| { return b; });
    }

    pub fn ignore_right<A>(self: Parser<T>, other: Parser<A>) -> Parser<T>
    where
        A: 'static,
        T: 'static,
    {
        let both = self.then(other);
        return both.map( |(a, _b)| { return a; });
    }

    pub fn between<A, B, C>(parser1: Parser<A>, parser2: Parser<B>, parser3: Parser<C>) -> Parser<B>
    where 
        A: 'static,
        B: 'static,
        C: 'static,
    {
        return parser1.ignore_left(parser2).ignore_right(parser3);
    }

    pub fn sep_by1<A>(self, separator: Parser<A>) -> Parser<Vec<T>>
    where 
        A: 'static,
        T: 'static,
    {
        let sep_then = separator.ignore_left(self.clone());

        return self.then(sep_then.many()).map(|(value, mut list)| { 
            list.insert(0, value);
            return list;
        });
    }

    pub fn sep_by<A>(self, separator: Parser<A>) -> Parser<Vec<T>>
    where
        A: 'static,
        T: Clone + 'static,
    {
        return self.sep_by1(separator).or(Parser::<Vec<T>>::returnp(Vec::<T>::new()));
    }

    pub fn any(expected_chars: impl IntoIterator<Item = char>) -> Parser<char>
    {
        let chars: Vec<char> = expected_chars.into_iter().collect();
        let description = format!("one of {:?}", chars.iter().collect::<String>());

        Parser::<char>::satisfy(move |found| chars.contains(&found), description)
    }

    /// Matches a single character satisfying `predicate`.
    ///
    /// Use this where the accepted set is easier to describe by rule than by
    /// listing it out, e.g. "any character except a quote". `description`
    /// names the expected input in the error message.
    pub fn satisfy<F>(predicate: F, description: impl Into<String>) -> Parser<char>
    where
        F: Fn(char) -> bool + 'static,
    {
        let description = description.into();

        Parser::new(move |input| {
            match input.chars().next() {
                Some(c) if predicate(c) => {
                    Ok((c, &input[c.len_utf8()..]))
                }
                _ => Err(ParseError::new(
                    input,
                    format!("Expected {}, found {}", description, describe_next(input)),
                )),
            }
        })
    }

    fn parse_zero_or_more<U>(parser: Parser<U>, input: &str) -> (Vec<U>, &str)
    {
        let first_result = parser.parse(input);

        match first_result {
            Ok(success) => {
                let first_value = success.0;
                let input_after_first = success.1;

                let (mut subsequent_values, remaining) = Parser::<T>::parse_zero_or_more(parser, input_after_first);
                subsequent_values.insert(0, first_value);

                return (subsequent_values, remaining);
            }
            Err(_error) => {
                return (Vec::new(), input);
            }
        }
    }

    pub fn choice<U>(parsers: impl IntoIterator<Item = Parser<U>>) -> Parser<U>
    where
        U: 'static
    {
        let parsers_col: Vec<Parser<U>> = parsers.into_iter().collect();

        Parser::new(move |input| {
            let mut best_error: Option<ParseError> = None;

            for parser in &parsers_col {
                match parser.parse(input) {
                    Ok(success) => { return Ok(success); }
                    Err(error) => {
                        best_error = Some(match best_error {
                            Some(previous) => previous.best(error),
                            None => error,
                        });
                    }
                }
            }

            return Err(best_error.unwrap_or_else(|| {
                ParseError::new(input, "No alternatives to choose from")
            }));
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

    pub fn apply_return<U>(self, value: U) -> Parser<U>
    where 
        T: 'static,
        U: Clone + 'static
    {
        return self.map(move |a| { return value.clone(); });
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
            match self.parse(input) {
                Ok(success) => Ok(success),
                Err(first) => other.parse(input).map_err(|second| first.best(second)),
            }
        })
    }

    /// Renames what this parser expects, for error messages.
    ///
    /// The new description only replaces the failure when nothing was
    /// consumed. Once a parser has committed to a branch, the deeper and more
    /// specific failure is the useful one, so it is left alone.
    pub fn label(self, description: impl Into<String>) -> Parser<T>
    where
        T: 'static,
    {
        let description = description.into();

        Parser::new(move |input| {
            self.parse(input).map_err(|error| {
                if error.remaining < input.len() { return error; }

                ParseError::new(
                    input,
                    format!("Expected {}, found {}", description, describe_next(input)),
                )
            })
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
