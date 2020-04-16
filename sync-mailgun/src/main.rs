mod api;
mod http;

use std::collections::{HashMap, HashSet};
use std::str;

use crate::api::Empty;
use curl::easy::{Form};
use failure::{bail, Error, ResultExt};
use rust_team_data::v1 as team_data;

const DESCRIPTION: &str = "managed by an automatic script on github";

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
    let mut mailmap = http::get::<team_data::Lists>(&api_url)?;

    // Mangle all the mailing list addresses
    for list in mailmap.lists.values_mut() {
        list.address = mangle_address(&list.address)?;
    }

    let mut routes = Vec::new();
    let mut response = http::get::<api::RoutesResponse>("/routes")?;
    let mut cur = 0;
    while response.items.len() > 0 {
        cur += response.items.len();
        routes.extend(response.items);
        if cur >= response.total_count {
            break
        }
        let url = format!("/routes?skip={}", cur);
        response = http::get::<api::RoutesResponse>(&url)?;
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

fn build_route_actions(list: &team_data::List) -> impl Iterator<Item = String> + '_ {
    list.members.iter().map(|member| format!("forward(\"{}\")", member))
}

fn create(new: &team_data::List) -> Result<(), Error> {
    let mut form = Form::new();
    form.part("priority").contents(b"0").add()?;
    form.part("description").contents(DESCRIPTION.as_bytes()).add()?;
    let expr = format!("match_recipient(\"{}\")", new.address);
    form.part("expression").contents(expr.as_bytes()).add()?;
    for action in build_route_actions(new) {
        form.part("action").contents(action.as_bytes()).add()?;
    }
    http::post::<Empty>("/routes", form)?;

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
    for action in build_route_actions(list) {
        form.part("action").contents(action.as_bytes()).add()?;
    }
    http::put::<Empty>(&format!("/routes/{}", route.id), form)?;

    Ok(())
}

fn del(route: &api::Route) -> Result<(), Error> {
    http::delete::<Empty>(&format!("/routes/{}", route.id))?;
    Ok(())
}

fn extract<'a>(s: &'a str, prefix: &str, suffix: &str) -> &'a str {
    assert!(s.starts_with(prefix), "`{}` didn't start with `{}`", s, prefix);
    assert!(s.ends_with(suffix), "`{}` didn't end with `{}`", s, suffix);
    &s[prefix.len()..s.len() - suffix.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_route_actions() {
        let list = team_data::List {
            address: "list@example.com".into(),
            members: vec![
                "foo@example.com".into(),
                "bar@example.com".into(),
                "baz@example.net".into(),
            ],
        };

        assert_eq!(vec![
            "forward(\"foo@example.com\")",
            "forward(\"bar@example.com\")",
            "forward(\"baz@example.net\")",
        ], build_route_actions(&list).collect::<Vec<_>>());
    }
}
