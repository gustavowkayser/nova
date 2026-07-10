pub enum Request {
    Method(String),
    Path(String),
    Values(Value),
}

pub enum Value {
    String(String),
    Number(i32),
    Boolean(bool),
    Block(Vec<Value>),
}