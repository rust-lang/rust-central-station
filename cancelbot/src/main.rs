extern crate curl;
extern crate futures;
extern crate getopts;
extern crate rustc_serialize;
extern crate time;
extern crate tokio_core;
extern crate tokio_curl;
#[macro_use]
extern crate error_chain;

use std::collections::HashMap;
use std::env;
use std::sync::Arc;
use std::time::Duration;

use errors::*;
use futures::Future;
use getopts::Options;
use tokio_core::reactor::{Core, Handle, Timeout};
use tokio_curl::Session;

macro_rules! t {
    ($e:expr) => {
        match $e {
            Ok(e) => e,
            Err(e) => panic!("{} failed with {}", stringify!($e), e),
        }
    };
}

type MyFuture<T> = Box<dyn Future<Item = T, Error = BorsError>>;

#[derive(Clone)]
struct State {
    travis_token: Option<String>,
    appveyor_token: Option<String>,
    azure_pipelines_token: Option<String>,
    session: Session,
    repos: Vec<Repo>,
    branch: String,
    appveyor_account_name: Option<String>,
    azure_pipelines_org: Option<String>,
}

#[derive(Clone)]
struct Repo {
    user: String,
    name: String,
}

mod appveyor;
mod azure;
mod errors;
mod http;
mod travis;

fn main() {
    let args = env::args().skip(1).collect::<Vec<_>>();
    let mut opts = Options::new();
    opts.reqopt("b", "branch", "branch to work with", "BRANCH");
    opts.optopt("t", "travis", "travis token", "TOKEN");
    opts.optopt("a", "appveyor", "appveyor token", "TOKEN");
    opts.optopt("", "appveyor-account", "appveyor account name", "ACCOUNT");
    opts.optopt("", "azure-pipelines-token", "", "TOKEN");
    opts.optopt("", "azure-pipelines-org", "", "ORGANIZATION");

    let usage = || -> ! {
        println!("{}", opts.usage("usage: ./foo -a ... -t ..."));
        std::process::exit(1);
    };

    let matches = match opts.parse(&args) {
        Ok(matches) => matches,
        Err(e) => {
            println!("error: {}", e);
            usage();
        }
    };

    let mut core = t!(Core::new());
    let handle = core.handle();

    let state = State {
        travis_token: matches.opt_str("t"),
        appveyor_token: matches.opt_str("a"),
        repos: matches
            .free
            .iter()
            .map(|m| {
                let mut parts = m.splitn(2, '/');
                Repo {
                    user: parts.next().unwrap().to_string(),
                    name: parts.next().unwrap().to_string(),
                }
            })
            .collect(),
        session: Session::new(handle.clone()),
        branch: matches.opt_str("b").unwrap(),
        appveyor_account_name: matches.opt_str("appveyor-account"),
        azure_pipelines_token: matches.opt_str("azure-pipelines-token"),
        azure_pipelines_org: matches.opt_str("azure-pipelines-org"),
    };

    core.run(state.check(&handle)).unwrap();
}

impl State {
    fn check(&self, handle: &Handle) -> MyFuture<()> {
        println!(
            "--------------------------------------------------------\n\
             {} - starting check",
            time::now().rfc822z()
        );
        let travis = self.check_travis();
        let travis = travis.then(|result| {
            println!("travis result {:?}", result);
            Ok(())
        });
        let appveyor = self.check_appveyor();
        let appveyor = appveyor.then(|result| {
            println!("appveyor result {:?}", result);
            Ok(())
        });
        let azure_pipelines = self.check_azure_pipelines();
        let azure_pipelines = azure_pipelines.then(|result| {
            println!("azure_pipelines result {:?}", result);
            Ok(())
        });

        let requests = travis
            .join(appveyor)
            .map(|_| ())
            .join(azure_pipelines)
            .map(|_| ());
        let timeout = t!(Timeout::new(Duration::new(30, 0), handle));
        Box::new(
            requests
                .map(Ok)
                .select(timeout.map(Err).map_err(From::from))
                .then(|res| match res {
                    Ok((Ok(()), _timeout)) => Ok(()),
                    Ok((Err(_), _requests)) => {
                        println!("timeout, canceling requests");
                        Ok(())
                    }
                    Err((e, _other)) => Err(e),
                }),
        )
    }

    fn check_travis(&self) -> MyFuture<()> {
        let futures = if let Some(token) = &self.travis_token {
            let token = Arc::new(token.clone());
            self.repos
                .iter()
                .map(|repo| self.check_travis_repo(repo.clone(), token.clone()))
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        Box::new(futures::collect(futures).map(|_| ()))
    }

    fn check_travis_repo(&self, repo: Repo, token: Arc<String>) -> MyFuture<()> {
        let url = format!("/repos/{}/{}/builds", repo.user, repo.name);
        let history = http::travis_get(&self.session, &url, &token);

        let me = self.clone();
        let cancel_old = history.and_then(move |list: travis::GetBuilds| {
            let mut futures = Vec::new();
            let commits = list
                .commits
                .iter()
                .map(|c| (c.id, c))
                .collect::<HashMap<_, _>>();

            // we're only interested in builds that concern our branch
            let builds = list
                .builds
                .iter()
                .filter(|build| match commits.get(&build.commit_id) {
                    Some(c) if c.branch != me.branch => false,
                    Some(_) => true,
                    None => false,
                })
                .collect::<Vec<_>>();

            // figure out what the max build number is, then cancel everything
            // that came before that.
            let max = builds
                .iter()
                .map(|b| b.number.parse::<usize>().unwrap())
                .max();
            for build in builds.iter() {
                if !me.travis_build_running(build) {
                    continue;
                }
                if build.number == max.unwrap_or(0).to_string() {
                    futures.push(me.travis_cancel_if_jobs_failed(build, token.clone()));
                } else {
                    println!(
                        "travis cancelling {} in {} as it's not the latest",
                        build.number, build.state
                    );
                    futures.push(me.travis_cancel_build(build, token.clone()));
                }
            }
            futures::collect(futures)
        });

        Box::new(cancel_old.map(|_| ()))
    }

    fn travis_cancel_if_jobs_failed(
        &self,
        build: &travis::Build,
        token: Arc<String>,
    ) -> MyFuture<()> {
        let url = format!("/builds/{}", build.id);
        let build = http::travis_get(&self.session, &url, &token);
        let me = self.clone();
        let cancel = build.and_then(move |b: travis::GetBuild| {
            let cancel = b.jobs.iter().any(|job| match &job.state[..] {
                "failed" | "errored" | "canceled" => true,
                _ => false,
            });

            if cancel {
                println!("cancelling top build {} as a job failed", b.build.number);
                me.travis_cancel_build(&b.build, token)
            } else {
                Box::new(futures::finished(()))
            }
        });

        Box::new(cancel.map(|_| ()))
    }

    fn travis_build_running(&self, build: &travis::Build) -> bool {
        match &build.state[..] {
            "passed" | "failed" | "canceled" | "errored" => false,
            _ => true,
        }
    }

    fn travis_cancel_build(&self, build: &travis::Build, token: Arc<String>) -> MyFuture<()> {
        let url = format!("/builds/{}/cancel", build.id);
        http::travis_post(&self.session, &url, &token)
    }

    fn check_appveyor(&self) -> MyFuture<()> {
        let futures = if let Some(token) = &self.appveyor_token {
            let token = Arc::new(token.clone());
            self.repos
                .iter()
                .map(|repo| self.check_appveyor_repo(repo.clone(), token.clone()))
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        Box::new(futures::collect(futures).map(|_| ()))
    }

    fn check_appveyor_repo(&self, repo: Repo, token: Arc<String>) -> MyFuture<()> {
        let url = format!(
            "/projects/{}/{}/history?recordsNumber=10&branch={}",
            self.appveyor_account_name.as_ref().unwrap_or(&repo.name),
            repo.name,
            self.branch
        );
        let history = http::appveyor_get(&self.session, &url, &token);

        let me = self.clone();
        let repo2 = repo.clone();
        let token2 = token.clone();
        let cancel_old = history.and_then(move |history: appveyor::History| {
            let max = history.builds.iter().map(|b| b.buildNumber).max();
            let mut futures = Vec::new();
            for build in history.builds.iter() {
                if !me.appveyor_build_running(build) {
                    continue;
                }
                if build.buildNumber < max.unwrap_or(0) {
                    println!(
                        "appveyor cancelling {} as it's not the latest",
                        build.buildNumber
                    );
                    futures.push(me.appveyor_cancel_build(&repo2, build, token2.clone()));
                }
            }
            futures::collect(futures)
        });

        let me = self.clone();
        let url = format!(
            "/projects/{}/{}/branch/{}",
            self.appveyor_account_name.as_ref().unwrap_or(&repo.name),
            repo.name,
            self.branch
        );
        let last_build = http::appveyor_get(&self.session, &url, &token);
        let me = me.clone();
        let cancel_if_failed = last_build.and_then(move |last: appveyor::LastBuild| {
            if !me.appveyor_build_running(&last.build) {
                return Box::new(futures::finished(())) as Box<_>;
            }
            for job in last.build.jobs.iter() {
                match &job.status[..] {
                    "success" | "queued" | "starting" | "running" => continue,
                    _ => {}
                }

                println!(
                    "appveyor cancelling {} as a job is {}",
                    last.build.buildNumber, job.status
                );
                return me.appveyor_cancel_build(&repo, &last.build, token);
            }
            Box::new(futures::finished(()))
        });

        Box::new(cancel_old.join(cancel_if_failed).map(|_| ()))
    }

    fn appveyor_build_running(&self, build: &appveyor::Build) -> bool {
        match &build.status[..] {
            "failed" | "cancelled" | "success" => false,
            _ => true,
        }
    }

    fn appveyor_cancel_build(
        &self,
        repo: &Repo,
        build: &appveyor::Build,
        token: Arc<String>,
    ) -> MyFuture<()> {
        let url = format!(
            "/builds/{}/{}/{}",
            self.appveyor_account_name.as_ref().unwrap_or(&repo.name),
            repo.name,
            build.version
        );
        http::appveyor_delete(&self.session, &url, &token)
    }

    fn check_azure_pipelines(&self) -> MyFuture<()> {
        let futures = if let Some(token) = &self.azure_pipelines_token {
            let token = Arc::new(token.clone());
            self.repos
                .iter()
                .map(|repo| self.check_azure_pipelines_repo(repo.clone(), token.clone()))
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        Box::new(futures::collect(futures).map(|_| ()))
    }

    fn check_azure_pipelines_repo(&self, repo: Repo, token: Arc<String>) -> MyFuture<()> {
        let url = format!(
            "/{}/{}/_apis/build/builds?api-version=5.0&repositoryType=GitHub&repositoryId={}/{}&branchName=refs/heads/{}",
            self.azure_pipelines_org.as_ref().unwrap_or(&repo.user),
            repo.name,
            repo.user,
            repo.name,
            self.branch,
        );
        let history = http::azure_pipelines_get(&self.session, &url, &token);

        let me = self.clone();
        let repo2 = repo.clone();
        let cancel_old = history.and_then(move |list: azure::List| {
            let max = list.value.iter().map(|b| b.id).max();
            let mut futures = Vec::new();
            for (i, build) in list.value.iter().enumerate() {
                if !me.azure_build_running(build) {
                    continue;
                }
                if build.id < max.unwrap_or(0) {
                    println!("azure cancelling {} as it's not the latest", build.id);
                    futures.push(me.azure_cancel_build(&repo2, build, token.clone()));
                    continue;
                }
                if i != 0 {
                    continue;
                }

                // If this is the first build look at the timeline (jobs) and if
                // anything failed then cancel the job.
                let timeline =
                    http::azure_pipelines_get(&me.session, &build._links.timeline.href, &token);
                let repo3 = repo2.clone();
                let me2 = me.clone();
                let build = build.clone();
                let token2 = token.clone();
                let cancel_first = timeline.and_then(move |list: azure::Timeline| {
                    if list.records.iter().any(|r| {
                        r.result.as_ref().map(|s| s == "failed").unwrap_or(false)
                            && r.r#type == "Job"
                    }) {
                        me2.azure_cancel_build(&repo3, &build, token2)
                    } else {
                        Box::new(futures::future::ok(()))
                    }
                });
                futures.push(Box::new(cancel_first));
            }
            futures::collect(futures)
        });

        Box::new(cancel_old.map(|_| ()))
    }

    fn azure_build_running(&self, build: &azure::Build) -> bool {
        match &build.status[..] {
            "cancelling" | "completed" => false,
            _ => true,
        }
    }

    fn azure_cancel_build(
        &self,
        repo: &Repo,
        build: &azure::Build,
        token: Arc<String>,
    ) -> MyFuture<()> {
        let url = format!(
            "/{}/{}/_apis/build/builds/{}?api-version=5.0",
            self.azure_pipelines_org.as_ref().unwrap_or(&repo.user),
            repo.name,
            build.id,
        );
        let body = "{\"status\":\"Cancelling\"}";
        http::azure_patch(&self.session, &url, &token, body)
    }
}
