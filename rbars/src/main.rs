extern crate toml;
extern crate handlebars;
extern crate rustc_serialize;

use std::fs::File;
use std::io::Read;
use std::env;

use handlebars::Handlebars;
use rustc_serialize::json::Json;
use toml::Value;

macro_rules! t {
    ($e:expr) => (match $e {
        Ok(e) => e,
        Err(e) => panic!("{} failed with {:?}", stringify!($e), e),
    })
}

fn main() {
    let mut args = env::args().skip(1);
    let config_file = args.next().unwrap();
    let template_file = args.next().unwrap();

    let mut config = String::new();
    t!(t!(File::open(&config_file)).read_to_string(&mut config));
    let mut template = String::new();
    t!(t!(File::open(&template_file)).read_to_string(&mut template));

	let mut handlebars = Handlebars::new();
	t!(handlebars.register_template_string("template", &template));

    let data = convert(t!(config.parse()));
	let data = t!(handlebars.render("template", &data));
    println!("{}", data);
}

fn convert(toml: Value) -> Json {
    match toml {
        Value::String(s) => Json::String(s),
        Value::Integer(i) => Json::I64(i),
        Value::Float(f) => Json::F64(f),
        Value::Boolean(b) => Json::Boolean(b),
        Value::Array(arr) => Json::Array(arr.into_iter().map(convert).collect()),
        Value::Table(table) => Json::Object(table.into_iter().map(|(k, v)| {
            (k, convert(v))
        }).collect()),
        Value::Datetime(dt) => Json::String(dt.to_string()),
    }
}
