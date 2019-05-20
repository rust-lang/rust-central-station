#![allow(bad_style)]

#[derive(RustcDecodable, Debug)]
pub struct History {
    pub project: Project,
    pub builds: Vec<Build>,
}

#[derive(RustcDecodable, Debug)]
pub struct Project {
    pub projectId: u32,
    pub accountId: u32,
    pub accountName: String,
    pub name: String,
    pub slug: String,
    pub repositoryName: String,
    pub repositoryType: String,
}

#[derive(RustcDecodable, Debug)]
pub struct Build {
    pub buildId: u32,
    pub jobs: Vec<Job>,
    pub buildNumber: u32,
    pub version: String,
    pub message: String,
    pub branch: String,
    pub commitId: String,
    pub status: String,
    pub started: Option<String>,
    pub finished: Option<String>,
    pub created: String,
    pub updated: Option<String>,
}

#[derive(RustcDecodable, Debug)]
pub struct Job {
    pub jobId: String,
    pub status: String,
}

#[derive(RustcDecodable, Debug)]
pub struct LastBuild {
    pub build: Build,
}
