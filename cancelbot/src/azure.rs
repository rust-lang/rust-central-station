#![allow(bad_style)]

#[derive(RustcDecodable, Debug)]
pub struct List {
    pub value: Vec<Build>,
}

#[derive(RustcDecodable, Debug, Clone)]
pub struct Build {
    pub id: u32,
    pub status: String,
    pub _links: BuildLinks,
}

#[derive(RustcDecodable, Debug, Clone)]
pub struct BuildLinks {
    pub timeline: Link,
}

#[derive(RustcDecodable, Debug, Clone)]
pub struct Link {
    pub href: String,
}

#[derive(RustcDecodable, Debug)]
pub struct Timeline {
    pub records: Vec<Record>,
}

#[derive(RustcDecodable, Debug)]
pub struct Record {
    pub name: String,
    pub result: Option<String>,
    pub r#type: String,
}
