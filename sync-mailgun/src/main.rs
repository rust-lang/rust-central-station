use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::env;
use std::str;

use curl::easy::{Easy, Form};
use failure::{bail, format_err, Error, ResultExt};
use rust_team_data::v1 as team_data;

const DESCRIPTION: &str = "managed by an automatic script on github";

mod api {
    #[derive(serde_derive::Deserialize)]
    pub struct ListResponse {
        pub items: Vec<List>,
        pub paging: Paging,
    }

    #[derive(serde_derive::Deserialize)]
    pub struct RoutesResponse {
        pub items: Vec<Route>,
        pub total_count: usize,
    }
    #[derive(serde_derive::Deserialize)]
    pub struct Route {
        pub actions: Vec<String>,
        pub expression: String,
        pub id: String,
        pub description: serde_json::Value,
    }

    #[derive(serde_derive::Deserialize)]
    pub struct List {
        pub access_level: String,
        pub address: String,
        pub members_count: u64,
    }

    #[derive(serde_derive::Deserialize)]
    pub struct Paging {
        pub first: String,
        pub last: String,
        pub next: String,
        pub previous: String,
    }

    #[derive(serde_derive::Deserialize)]
    pub struct MembersResponse {
        pub items: Vec<Member>,
        pub paging: Paging,
    }

    #[derive(serde_derive::Deserialize)]
    pub struct Member {
        pub address: String,
    }
}

#[derive(serde_derive::Deserialize)]
struct Empty {}

fn main() {
    env_logger::init();
    if let Err(e) = run() {
        eprintln!("error: {}", e);
        for e in e.iter_causes() {
            eprintln!("  cause: {}", e);
        }
        std::process::exit(1);
    }
}

fn mangle_address(addr: &str) -> Result<String, Error> {
    // Escape dots since they have a special meaning in Python regexes
    let mangled = addr.replace(".", "\\.");

    // Inject (?:\+.+)? before the '@' in the address to support '+' aliases like
    // infra+botname@rust-lang.org
    if let Some(at_pos) = mangled.find('@') {
        let (user, domain) = mangled.split_at(at_pos);
        Ok(format!("^{}(?:\\+.+)?{}$", user, domain))
    } else {
        bail!("the address `{}` doesn't have any '@'", addr);
    }
}

fn run() -> Result<(), Error> {
    let api_url = if let Ok(url) = std::env::var("TEAM_DATA_BASE_URL") {
        format!("{}/lists.json", url)
    } else {
        format!("{}/lists.json", team_data::BASE_URL)
    };
    let mut mailmap = get::<team_data::Lists>(&api_url)?;

    // Mangle all the mailing list addresses
    for list in mailmap.lists.values_mut() {
        list.address = mangle_address(&list.address)?;
    }

    let mut routes = Vec::new();
    let mut response = get::<api::RoutesResponse>("/routes")?;
    let mut cur = 0;
    while response.items.len() > 0 {
        cur += response.items.len();
        routes.extend(response.items);
        if cur >= response.total_count {
            break
        }
        let url = format!("/routes?skip={}", cur);
        response = get::<api::RoutesResponse>(&url)?;
    }

    let mut addr2list = HashMap::new();
    for list in mailmap.lists.values() {
        if addr2list.insert(&list.address[..], list).is_some() {
            bail!("duplicate address: {}", list.address);
        }
    }

    for route in routes {
        if route.description != DESCRIPTION {
            continue
        }
        let address = extract(&route.expression, "match_recipient(\"", "\")");
        match addr2list.remove(address) {
            Some(new_list) => {
                sync(&route, &new_list)
                    .with_context(|_| format!("failed to sync {}", address))?
            }
            None => {
                del(&route)
                    .with_context(|_| format!("failed to delete {}", address))?
            }
        }
    }

    for (_, list) in addr2list.iter() {
        create(list)
            .with_context(|_| format!("failed to create {}", list.address))?;
    }

    Ok(())
}

fn create(new: &team_data::List) -> Result<(), Error> {
    let mut form = Form::new();
    form.part("priority").contents(b"0").add()?;
    form.part("description").contents(DESCRIPTION.as_bytes()).add()?;
    let expr = format!("match_recipient(\"{}\")", new.address);
    form.part("expression").contents(expr.as_bytes()).add()?;
    for member in new.members.iter() {
        form.part("action").contents(format!("forward(\"{}\")", member).as_bytes()).add()?;
    }
    post::<Empty>("/routes", form)?;

    Ok(())
}

fn sync(route: &api::Route, list: &team_data::List) -> Result<(), Error> {
    let before = route
        .actions
        .iter()
        .map(|action| extract(action, "forward(\"", "\")"))
        .collect::<HashSet<_>>();
    let after = list.members.iter().map(|s| &s[..]).collect::<HashSet<_>>();
    if before == after {
        return Ok(())
    }

    let mut form = Form::new();
    for member in list.members.iter() {
        form.part("action").contents(format!("forward(\"{}\")", member).as_bytes()).add()?;
    }
    put::<Empty>(&format!("/routes/{}", route.id), form)?;

    Ok(())
}

fn del(route: &api::Route) -> Result<(), Error> {
    delete::<Empty>(&format!("/routes/{}", route.id))?;
    Ok(())
}

fn get<T: for<'de> serde::Deserialize<'de>>(url: &str) -> Result<T, Error> {
    execute(url, Method::Get)
}

fn post<T: for<'de> serde::Deserialize<'de>>(
    url: &str,
    form: Form,
) -> Result<T, Error> {
    execute(url, Method::Post(form))
}

fn put<T: for<'de> serde::Deserialize<'de>>(
    url: &str,
    form: Form,
) -> Result<T, Error> {
    execute(url, Method::Put(form))
}

fn delete<T: for<'de> serde::Deserialize<'de>>(url: &str) -> Result<T, Error> {
    execute(url, Method::Delete)
}

enum Method {
    Get,
    Delete,
    Post(Form),
    Put(Form),
}

fn execute<T: for<'de> serde::Deserialize<'de>>(
    url: &str,
    method: Method,
) -> Result<T, Error> {
    thread_local!(static HANDLE: RefCell<Easy> = RefCell::new(Easy::new()));
    let password = env::var("MAILGUN_API_TOKEN")
        .map_err(|_| format_err!("must set $MAILGUN_API_TOKEN"))?;
    let result = HANDLE.with(|handle| {
        let mut handle = handle.borrow_mut();
        handle.reset();
        let url = if url.starts_with("http://") || url.starts_with("https://") {
            url.to_string()
        } else {
            format!("https://api.mailgun.net/v3{}", url)
        };
        handle.url(&url)?;
        match method {
            Method::Get => {
                log::debug!("GET {}", url);
                handle.get(true)?;
            }
            Method::Delete => {
                log::debug!("DELETE {}", url);
                handle.custom_request("DELETE")?;
            }
            Method::Post(form) => {
                log::debug!("POST {}", url);
                handle.httppost(form)?;
            }
            Method::Put(form) => {
                log::debug!("PUT {}", url);
                handle.httppost(form)?;
                handle.custom_request("PUT")?;
            }
        }
        // Add the API key only for Mailgun requests
        if url.starts_with("https://api.mailgun.net") {
            handle.username("api")?;
            handle.password(&password)?;
        }
        handle.useragent("rust-lang/rust membership update")?;
        // handle.verbose(true)?;
        let mut result = Vec::new();
        let mut headers = Vec::new();
        {
            let mut transfer = handle.transfer();
            transfer.write_function(|data| {
                result.extend_from_slice(data);
                Ok(data.len())
            })?;
            transfer.header_function(|header| {
                if let Ok(s) = str::from_utf8(header) {
                    headers.push(s.to_string());
                }
                true
            })?;
            transfer.perform()?;
        }

        let result = String::from_utf8(result)
            .map_err(|_| format_err!("response was invalid utf-8"))?;

        log::trace!("headers: {:#?}", headers);
        log::trace!("json: {}", result);
        let code = handle.response_code()?;
        if code != 200 {
            bail!("failed to get a 200 code, got {}\n\n{}", code, result)
        }
        Ok(serde_json::from_str(&result)
            .with_context(|_| "failed to parse json response")?)
    });
    Ok(result.with_context(|_| format!("failed to send request to {}", url))?)
}

fn extract<'a>(s: &'a str, prefix: &str, suffix: &str) -> &'a str {
    assert!(s.starts_with(prefix), "`{}` didn't start with `{}`", s, prefix);
    assert!(s.ends_with(suffix), "`{}` didn't end with `{}`", s, suffix);
    &s[prefix.len()..s.len() - suffix.len()]
}
