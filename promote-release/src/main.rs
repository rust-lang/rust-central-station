extern crate fs2;
extern crate toml;
extern crate curl;
extern crate tar;
extern crate flate2;

use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{PathBuf, Path};
use std::process::Command;

use curl::easy::Easy;
use fs2::FileExt;

macro_rules! t {
    ($e:expr) => (match $e {
        Ok(e) => e,
        Err(e) => panic!("{} failed with {:?}", stringify!($e), e),
    })
}

struct Context {
    work: PathBuf,
    release: String,
    handle: Easy,
	secrets: toml::Value,
    date: String,
}

fn main() {
    let mut secrets = String::new();
    t!(t!(File::open(env::args().nth(3).unwrap())).read_to_string(&mut secrets));

    Context {
        work: t!(env::current_dir()).join(env::args_os().nth(1).unwrap()),
        release: env::args().nth(2).unwrap(),
        secrets: t!(secrets.parse()),
        handle: Easy::new(),
        date: output(Command::new("date").arg("+%Y-%m-%d")),
    }.run()
}

impl Context {
    fn run(&mut self) {
        let _lock = self.lock();
        self.update_repo();
        let branch = match &self.release[..] {
            "nightly" => "master",
            "beta" => "beta",
            "stable" => "stable",
            _ => panic!("unknown release: {}", self.release),
        };
        self.do_release(branch);
    }

    fn lock(&mut self) -> File {
        t!(fs::create_dir_all(&self.work));
        let file = t!(OpenOptions::new()
                            .read(true)
                            .write(true)
                            .create(true)
                            .open(self.work.join(".lock")));
        t!(file.try_lock_exclusive());
        file
    }

    fn update_repo(&mut self) {
        // Clone/update the repo
        let dir = self.rust_dir();
        if dir.is_dir() {
            println!("fetching");
            run(Command::new("git")
                        .arg("fetch")
                        .arg("origin")
                        .current_dir(&dir));
        } else {
            println!("cloning");
            run(Command::new("git")
                        .arg("clone")
                        .arg("https://github.com/rust-lang/rust")
                        .arg(&dir));
        }
    }

    fn do_release(&mut self, branch: &str) {
        let rev = output(Command::new("git")
                                 .arg("rev-parse")
                                 .arg(format!("origin/{}", branch))
                                 .current_dir(&self.rust_dir()));
        let rev = rev.trim();
        println!("{} rev is {}", self.release, rev);

        self.configure_rust(rev);

        let manifest = self.download_manifest();
        let previous_version = manifest.lookup("pkg.rust.version")
                                       .expect("rust version not present")
                                       .as_str()
                                       .expect("rust version not a string");
        println!("previous version: {}", previous_version);

        if previous_version.contains(&rev[..7]) && false {
            return println!("found rev in previous version, skipping");
        }

        self.download_artifacts(&rev);

        if !self.version_changed_since(&previous_version) {
            return println!("version hasn't changed, skipping");
        }

        self.sign_artifacts();
        self.upload_signatures(&rev);

        for file in t!(self.build_dir().join("build/dist/").read_dir()) {
            let file = t!(file);
            t!(fs::copy(file.path(), self.dl_dir().join(file.file_name())));
        }

        self.publish_archive();
        self.publish_release();
    }

    fn configure_rust(&mut self, rev: &str) {
        let build = self.build_dir();
        drop(fs::remove_dir_all(&build));
        t!(fs::create_dir_all(&build));
        let rust = self.rust_dir();

        run(Command::new("git")
                    .arg("reset")
                    .arg("--hard")
                    .arg(rev)
                    .current_dir(&rust));

        run(Command::new(rust.join("configure"))
                    .current_dir(&build)
                    .arg(format!("--release-channel={}", self.release)));
        t!(t!(File::create(build.join("config.toml"))).write_all(format!("\
[dist]
sign-folder = \"{}\"
gpg-password-file = \"{}\"
upload-addr = \"{}\"
",
            self.dl_dir().display(),
            self.secrets.lookup("dist.gpg-password-file").unwrap()
                        .as_str().unwrap(),
            self.secrets.lookup("dist.upload-addr").unwrap()
                        .as_str().unwrap()).as_bytes()));
    }

    fn version_changed_since(&mut self, prev: &str) -> bool {
        // nightly's always changing
        if self.release == "nightly" {
            return true
        }
        let prev_version = prev.split(' ').next().unwrap();

        let current = t!(self.dl_dir().read_dir()).filter_map(|e| {
            let e = t!(e);
            let filename = e.file_name().into_string().unwrap();
            if !filename.starts_with("rustc-") || !filename.ends_with(".tar.gz") {
                return None
            }
            println!("looking inside {} for a version", filename);

            let file = t!(File::open(&e.path()));
            let reader = t!(flate2::read::GzDecoder::new(file));
            let mut archive = tar::Archive::new(reader);

            let entry = t!(archive.entries()).map(|e| t!(e)).filter(|e| {
                let path = t!(e.path());
                match path.iter().skip(1).next() {
                    Some(path) => path == Path::new("version"),
                    None => false,
                }
            }).next();
            let mut entry = match entry {
                Some(e) => e,
                None => return None,
            };
            let mut contents = String::new();
            t!(entry.read_to_string(&mut contents));
            Some(contents)
        }).next().expect("no archives with a version");

        println!("current version: {}", current);

        let current_version = current.split(' ').next().unwrap();

        prev_version != current_version
    }

    fn download_artifacts(&mut self, rev: &str) {
        let dl = self.dl_dir();
        t!(fs::create_dir_all(&dl));

        let src = format!("s3://rust-lang-ci/rustc-builds/{}/", rev);
        run(Command::new("s3cmd")
                    .arg("sync")
                    .arg("--delete-removed")
                    .arg(&src)
                    .arg(format!("{}/", dl.display())));

        let mut files = t!(dl.read_dir());
        if files.next().is_none() {
            panic!("appears that this rev doesn't have any artifacts, \
                    is this a stable/beta branch awaiting a PR?");
        }
    }

    fn sign_artifacts(&mut self) {
        let build = self.build_dir();
        run(Command::new(self.rust_dir().join("x.py"))
                    .current_dir(&build)
                    .arg("dist")
                    .arg("hash-and-sign"));
    }

    fn upload_signatures(&mut self, rev: &str) {
        let dst = format!("s3://rust-lang-ci/rustc-builds/{}/", rev);
        run(Command::new("s3cmd")
                    .arg("sync")
                    .arg("-n")
                    .arg(self.build_dir().join("build/dist/"))
                    .arg(&dst));
    }

    fn publish_archive(&mut self) {
        let bucket = self.secrets.lookup("dist.upload-bucket").unwrap()
                                 .as_str().unwrap();
        let dst = format!("s3://{}/dist/{}/", bucket, self.date);
        run(Command::new("s3cmd")
                    .arg("sync")
                    .arg("-n")
                    .arg(format!("{}/", self.dl_dir().display()))
                    .arg(&dst));
    }

    fn publish_release(&mut self) {
        let bucket = self.secrets.lookup("dist.upload-bucket").unwrap()
                                 .as_str().unwrap();
        let dst = format!("s3://{}/{}/", bucket, self.date);
        run(Command::new("s3cmd")
                    .arg("sync")
                    .arg("-n")
                    .arg(format!("{}/", self.dl_dir().display()))
                    .arg(&dst));
    }

    fn rust_dir(&self) -> PathBuf {
        self.work.join("rust")
    }

    fn dl_dir(&self) -> PathBuf {
        self.work.join("dl")
    }

    fn build_dir(&self) -> PathBuf {
        self.work.join("build")
    }

    fn download_manifest(&mut self) -> toml::Value {
        t!(self.handle.get(true));
        let url = format!("https://static.rust-lang.org/dist/channel-rust-{}.toml",
                          self.release);
        t!(self.handle.url(&url));
        let mut result = Vec::new();
        {
            let mut t = self.handle.transfer();

            t!(t.write_function(|data| {
                result.extend_from_slice(data);
                Ok(data.len())
            }));
            t!(t.perform());
        }
        assert_eq!(t!(self.handle.response_code()), 200);
        t!(t!(String::from_utf8(result)).parse())
    }
}

fn run(cmd: &mut Command) {
    println!("running {:?}", cmd);
    let status = t!(cmd.status());
    if !status.success() {
        panic!("failed command:{:?}\n:{}", cmd, status);
    }
}

fn output(cmd: &mut Command) -> String {
    println!("running {:?}", cmd);
    let output = t!(cmd.output());
    if !output.status.success() {
        panic!("failed command:{:?}\n:{}\n\n{}\n\n{}", cmd, output.status,
               String::from_utf8_lossy(&output.stdout),
               String::from_utf8_lossy(&output.stderr),);
    }

    String::from_utf8(output.stdout).unwrap()
}
