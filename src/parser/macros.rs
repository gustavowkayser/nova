
#[macro_export]
macro_rules! parse {
    ($final:ident) => {
        $final
    };

    ($left:ident .>>. $right:ident) => {
        crate::parser::parser::and_then($left, $right)
    };

    ($head:ident <|> $( $tail:tt )+) => {
        crate::parser::parser::or_else($head, parse!($( $tail )+))
    };
}