extern crate toml;

use std::io::{self, Read};
use std::env;

use toml::Value;

fn main() {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();

    let value: Value = input.parse().expect("failed to parse input");

    for arg in env::args().skip(1) {
        let mut value = &value;
        for part in arg.split('.') {
            value = &value[part];
        }
        match *value {
            Value::String(ref s) => println!("{}", s),
            Value::Integer(i) => println!("{}", i),
            Value::Float(f) => println!("{}", f),
            Value::Boolean(b) => println!("{}", b),
            Value::Datetime(ref s) => println!("{}", s),
            Value::Array(_) | Value::Table(_) => {
                panic!("cannot print array/table");
            }
        }
    }
}
