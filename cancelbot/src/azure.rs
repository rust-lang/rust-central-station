#![allow(bad_style)]

#[derive(RustcDecodable, Debug)]
pub struct List {
    pub value: Vec<Build>,
}

#[derive(RustcDecodable, Debug)]
pub struct Build {
    pub id: u32,
    pub status: String,
}
