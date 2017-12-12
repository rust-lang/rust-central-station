extern crate curl;
extern crate flate2;
extern crate fs2;
extern crate rand;
#[macro_use]
extern crate serde_json;
extern crate tar;
extern crate toml;

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
    current_version: Option<String>,
}

// Called as:
//
//  $prog work/dir release-channel path/to/secrets.toml
fn main() {
    let mut secrets = String::new();
    t!(t!(File::open(env::args().nth(3).unwrap())).read_to_string(&mut secrets));

    Context {
        work: t!(env::current_dir()).join(env::args_os().nth(1).unwrap()),
        release: env::args().nth(2).unwrap(),
        secrets: t!(secrets.parse()),
        handle: Easy::new(),
	// For Nightly, this is running soon after midnight
	// so the date of 10 minutes ago is yesterday,
	// which likely matches the commit date of the latest commit on the master branch:
	// https://github.com/rust-lang/rust-central-station/pull/27
        date: output(Command::new("date").arg("--date=10 minutes ago").arg("+%Y-%m-%d"))
	    .trim().to_string(),
        current_version: None,
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

    /// Locks execution of concurrent invocations of this script in case one
    /// takes a long time to run. The call to `try_lock_exclusive` will fail if
    /// the lock is held already
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

    /// Update the rust repository we have cached, either cloning a fresh one or
    /// fetching remote references
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

    /// Does a release for the `branch` specified.
    fn do_release(&mut self, branch: &str) {
        // Learn the precise rev of the remote branch, this'll guide what we
        // download.
        let rev = output(Command::new("git")
                                 .arg("rev-parse")
                                 .arg(format!("origin/{}", branch))
                                 .current_dir(&self.rust_dir()));
        let rev = rev.trim();
        println!("{} rev is {}", self.release, rev);

        // Download the current live manifest for the channel we're releasing.
        // Through that we learn the current version of the release.
        let manifest = self.download_manifest();
        let previous_version = manifest["pkg"]["rust"]["version"]
                                       .as_str()
                                       .expect("rust version not a string");
        println!("previous version: {}", previous_version);

        // If the previously released version is the same rev, then there's
        // nothing for us to do, nothing has changed.
        if previous_version.contains(&rev[..7]) {
            return println!("found rev in previous version, skipping");
        }

        // We may still not do a release if the version number hasn't changed.
        // To learn about the current branch's version number we download
        // artifacts and look inside.
        //
        // If revisions of the current release and the current branch are
        // different and the versions are the same then there's nothing for us
        // to do. This represents a scenario where changes have been merged to
        // the stable/beta branch but the version bump hasn't happened yet.
        self.download_artifacts(&rev);
        if self.current_version_same(&previous_version) {
            return println!("version hasn't changed, skipping");
        }

        // Ok we've now determined that a release needs to be done. Let's
        // configure rust, sign the artifacts we just downloaded, and upload the
        // signatures to the CI bucket.
        self.configure_rust(rev);
        self.sign_artifacts();
        self.upload_signatures(&rev);

        // Merge all the signatures with the download files, and then sync that
        // whole dir up to the release archives
        for file in t!(self.build_dir().join("build/dist/").read_dir()) {
            let file = t!(file);
            t!(fs::copy(file.path(), self.dl_dir().join(file.file_name())));
        }
        self.publish_archive();
        self.publish_docs();
        self.publish_release();

        self.update_dist_index();
        self.invalidate_cloudfront();
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
        let mut config = String::new();
        let path = build.join("config.toml");
        drop(File::open(&path).and_then(|mut f| f.read_to_string(&mut config)));
        let lines = config.lines().filter(|l| !l.starts_with("[dist]"));
        let mut new_config = String::new();
        for line in lines {
            new_config.push_str(line);
            new_config.push_str("\n");
        }
        new_config.push_str(&format!("
[dist]
sign-folder = \"{}\"
gpg-password-file = \"{}\"
upload-addr = \"{}/{}\"
",
            self.dl_dir().display(),
            self.secrets["dist"]["gpg-password-file"].as_str().unwrap(),
            self.secrets["dist"]["upload-addr"].as_str().unwrap(),
            self.secrets["dist"]["upload-dir"].as_str().unwrap()));
        t!(t!(File::create(&path)).write_all(new_config.as_bytes()));
    }

    fn current_version_same(&mut self, prev: &str) -> bool {
        // nightly's always changing
        if self.release == "nightly" {
            return false
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
        self.current_version = Some(current_version.to_string());

        // The release process for beta looks like so:
        //
        // * Force push master branch to beta branch
        // * Send a PR to beta, updating release channel
        //
        // In the window between these two steps we don't actually have release
        // artifacts but this script may be run. Try to detect that case here if
        // the versions mismatch and panic. We'll try again later once that PR
        // has merged and everything should look good.
        if (current.contains("nightly") && !prev.contains("nightly")) ||
           (current.contains("beta") && !prev.contains("beta")) {
            panic!("looks like channels are being switched -- was this branch \
                    just created and has a pending PR to change the release \
                    channel?");
        }

        prev_version == current_version
    }

    fn download_artifacts(&mut self, rev: &str) {
        let dl = self.dl_dir();
        drop(fs::remove_dir_all(&dl));
        t!(fs::create_dir_all(&dl));

        let src = format!("s3://rust-lang-ci2/rustc-builds/{}/", rev);
        run(self.aws_s3()
                .arg("cp")
                .arg("--recursive")
                .arg("--only-show-errors")
                .arg(&src)
                .arg(format!("{}/", dl.display())));

        let mut files = t!(dl.read_dir());
        if files.next().is_none() {
            panic!("appears that this rev doesn't have any artifacts, \
                    is this a stable/beta branch awaiting a PR?");
        }

        // Delete residue signature/hash files. These may come around for a few
        // reasons:
        //
        // 1. We died halfway through before uploading the manifest, in which
        //    case we want to re-upload everything but we don't want to sign
        //    signatures.
        //
        // 2. We're making a stable release. The stable release is first signed
        //    with the dev key and then it's signed with the prod key later. We
        //    want the prod key to overwrite the dev key signatures.
        for file in t!(dl.read_dir()) {
            let file = t!(file);
            let path = file.path();
            match path.extension().and_then(|s| s.to_str()) {
                Some("asc") |
                Some("sha256") => {
                    t!(fs::remove_file(&path));
                }
                _ => {}
            }
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
        let dst = format!("s3://rust-lang-ci2/rustc-builds/{}/", rev);
        run(self.aws_s3()
                .arg("cp")
                .arg("--recursive")
                .arg("--only-show-errors")
                .arg(self.build_dir().join("build/dist/"))
                .arg(&dst));
    }

    fn publish_archive(&mut self) {
        let bucket = self.secrets["dist"]["upload-bucket"].as_str().unwrap();
        let dir = self.secrets["dist"]["upload-dir"].as_str().unwrap();
        let dst = format!("s3://{}/{}/{}/", bucket, dir, self.date);
        run(self.aws_s3()
                .arg("cp")
                .arg("--recursive")
                .arg("--only-show-errors")
                .arg(format!("{}/", self.dl_dir().display()))
                .arg(&dst));
    }

    fn publish_docs(&mut self) {
        let (version, upload_dir) = match &self.release[..] {
            "stable" => {
                let vers = &self.current_version.as_ref().unwrap()[..];
                (vers, "stable")
            }
            "beta" => ("beta", "beta"),
            "nightly" => ("nightly", "nightly"),
            _ => panic!(),
        };

        // Pull out HTML documentation from one of the `rust-docs-*` tarballs.
        // For now we just arbitrarily pick x86_64-unknown-linux-gnu.
        let docs = self.work.join("docs");
        drop(fs::remove_dir_all(&docs));
        t!(fs::create_dir_all(&docs));
        let target = "x86_64-unknown-linux-gnu";
        let tarball_prefix = format!("rust-docs-{}-{}", version, target);
        let tarball = format!("{}.tar.gz",
                              self.dl_dir().join(&tarball_prefix).display());
        let tarball_dir = format!("{}/rust-docs/share/doc/rust/html",
                                  tarball_prefix);
        run(Command::new("tar")
                    .arg("xf")
                    .arg(&tarball)
                    .arg("--strip-components=6")
                    .arg(&tarball_dir)
                    .current_dir(&docs));

        // Upload this to `/doc/$channel`
        let bucket = self.secrets["dist"]["upload-bucket"].as_str().unwrap();
        let dst = format!("s3://{}/doc/{}/", bucket, upload_dir);
        run(self.aws_s3()
                .arg("sync")
                .arg("--delete")
                .arg("--only-show-errors")
                .arg(format!("{}/", docs.display()))
                .arg(&dst));

        // Stable artifacts also go to `/doc/$version/
        if upload_dir == "stable" {
            let dst = format!("s3://{}/doc/{}/", bucket, version);
            run(self.aws_s3()
                    .arg("sync")
                    .arg("--delete")
                    .arg("--only-show-errors")
                    .arg(format!("{}/", docs.display()))
                    .arg(&dst));
        }
    }

    fn publish_release(&mut self) {
        let bucket = self.secrets["dist"]["upload-bucket"].as_str().unwrap();
        let dir = self.secrets["dist"]["upload-dir"].as_str().unwrap();
        let dst = format!("s3://{}/{}/", bucket, dir);
        run(self.aws_s3()
                .arg("cp")
                .arg("--recursive")
                .arg("--only-show-errors")
                .arg(format!("{}/", self.dl_dir().display()))
                .arg(&dst));
    }

    fn update_dist_index(&mut self) {
        // cp config.toml-dist config.toml -f
        // python3 generate.py
        let dir = self.work.join("update-dist-index");
        drop(fs::remove_dir_all(&dir));
        t!(fs::create_dir_all(&dir));

        let config = self.work.join("s3-directory-listing.toml");

        t!(t!(File::create(&config)).write_all(format!("
[bucket]
# Must be specified:
name   = '{bucket}'
region = '{region}'
access_key = '{access_key}'
secret_key = '{secret_key}'

# Optional (default values listed):
prefix = '{upload_dir}'
path_separator = '/'
base_url = ''

[output.webindex]
type = 'html'
extra_head = ''
list_zero_sized = false
file_sort_key = 'name'
reverse_files = false

[output.macinereadable1]
type = 'json'
pretty = true

[output.delimitedvalues]
type = 'txt'
delimiter = ','
file_fields = ['path', 'size', 'mdate']
filename = 'index.txt'

",
        bucket = self.secrets["dist"]["upload-bucket"].as_str().unwrap(),
        region = self.secrets["dist"]["upload-bucket-region"].as_str().unwrap(),
        upload_dir = self.secrets["dist"]["upload-dir"].as_str().unwrap(),
        access_key = self.secrets["dist"]["aws-access-key-id"].as_str().unwrap(),
        secret_key = self.secrets["dist"]["aws-secret-key"].as_str().unwrap(),
).as_bytes()));

        run(Command::new("python3")
                    .arg("/s3-directory-listing/generate.py")
                    .arg("--output").arg(&dir)
                    .arg(&config));
        t!(fs::rename(dir.join("index.json"), dir.join("dist/index.json")));
        t!(fs::rename(dir.join("index.txt"), dir.join("dist/index.txt")));

        let bucket = self.secrets["dist"]["upload-bucket"].as_str().unwrap();
        let upload_dir = self.secrets["dist"]["upload-dir"].as_str().unwrap();
        let dst = format!("s3://{}/{}/", bucket, upload_dir);
        run(self.aws_s3()
                .arg("cp")
                .arg("--recursive")
                .arg("--only-show-errors")
                .arg(format!("{}/", dir.join("dist").display()))
                .arg(&dst));
    }

    fn invalidate_cloudfront(&mut self) {
        let json = json!({
            "Paths": {
                "Items": [
                    "/dist/channel*",
                    "/dist/rust*",
                    "/dist/index*",
                    "/dist/",
                ],
                "Quantity": 4,
            },
            "CallerReference": format!("rct-{}", rand::random::<usize>()),
        }).to_string();
        let dst = self.work.join("payload.json");
        t!(t!(File::create(&dst)).write_all(json.as_bytes()));

        let distribution_id = self.secrets["dist"]["cloudfront-distribution-id"]
                                          .as_str().unwrap();
        let mut cmd = Command::new("aws");
        self.aws_creds(&mut cmd);
        run(cmd.arg("cloudfront")
               .arg("create-invalidation")
               .arg("--invalidation-batch").arg(format!("file://{}", dst.display()))
               .arg("--distribution-id").arg(distribution_id));
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

    fn aws_s3(&self) -> Command {
        let mut cmd = Command::new("aws");
        cmd.arg("s3");
        self.aws_creds(&mut cmd);
        return cmd
    }

    fn aws_creds(&self, cmd: &mut Command) {
        let access = self.secrets["dist"]["aws-access-key-id"].as_str().unwrap();
        let secret = self.secrets["dist"]["aws-secret-key"].as_str().unwrap();
        cmd.env("AWS_ACCESS_KEY_ID", &access)
           .env("AWS_SECRET_ACCESS_KEY", &secret);
    }

    fn download_manifest(&mut self) -> toml::Value {
        t!(self.handle.get(true));
        let addr = self.secrets["dist"]["upload-addr"].as_str().unwrap();
        let upload_dir = self.secrets["dist"]["upload-dir"].as_str().unwrap();
        let url = format!("{}/{}/channel-rust-{}.toml",
                          addr,
                          upload_dir,
                          self.release);
        println!("downloading manifest from: {}", url);
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
