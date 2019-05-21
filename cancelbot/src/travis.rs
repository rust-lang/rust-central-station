#[derive(RustcDecodable, Debug)]
pub struct GetBuilds {
    pub builds: Vec<Build>,
    pub commits: Vec<Commit>,
}

#[derive(RustcDecodable, Debug)]
pub struct Build {
    pub id: u32,
    pub number: String,
    pub state: String,
    pub commit_id: u32,
    pub job_ids: Vec<u32>,
}

#[derive(RustcDecodable, Debug)]
pub struct Commit {
    pub id: u32,
    pub branch: String,
}

#[derive(RustcDecodable, Debug)]
pub struct GetBuild {
    pub commit: Commit,
    pub build: Build,
    pub jobs: Vec<Job>,
}

#[derive(RustcDecodable, Debug)]
pub struct Job {
    pub id: u32,
    pub build_id: u32,
    pub allow_failure: bool,
    pub state: String,
}
