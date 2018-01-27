extern crate toml;
extern crate handlebars;
extern crate serde_json;

use std::fs::File;
use std::io::Read;
use std::env;

use handlebars::Handlebars;
use serde_json::Value as Json;
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

// we cannot use `serde_json::to_value` because we want Datetime to be string.
fn convert(toml: Value) -> Json {
    match toml {
        Value::String(s) => Json::String(s),
        Value::Integer(i) => i.into(),
        Value::Float(f) => f.into(),
        Value::Boolean(b) => Json::Bool(b),
        Value::Array(arr) => Json::Array(arr.into_iter().map(convert).collect()),
        Value::Table(table) => Json::Object(table.into_iter().map(|(k, v)| {
            (k, convert(v))
        }).collect()),
        Value::Datetime(dt) => Json::String(dt.to_string()),
    }
}
