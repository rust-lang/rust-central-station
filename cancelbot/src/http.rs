use std::str;
use std::sync::{Arc, Mutex};

use curl::easy::{Easy, List};
use futures::Future;
use rustc_serialize::json;
use rustc_serialize::Decodable;
use tokio_curl::Session;

use errors::*;
use MyFuture;

static TRAVIS_API_BASE: &str = "https://api.travis-ci.com";
static APPVEYOR_API_BASE: &str = "https://ci.appveyor.com/api";
static AZURE_API_BASE: &str = "https://dev.azure.com";
static AGENT: &str = "User-Agent: cancelbot (github.com/rust-lang/rust-central-station)";

fn append_url(host: &str, url: &str) -> String {
    if url.starts_with("https://") {
        url.to_string()
    } else {
        format!("{}{}", host, url)
    }
}

#[allow(dead_code)]
pub struct Response {
    easy: Easy,
    headers: Arc<Mutex<Vec<Vec<u8>>>>,
    body: Arc<Mutex<Vec<u8>>>,
}

pub fn travis_get<T>(sess: &Session, url: &str, token: &str) -> MyFuture<T>
where
    T: Decodable + 'static,
{
    let url = append_url(TRAVIS_API_BASE, url);
    let headers = vec![
        format!("Authorization: token {}", token),
        format!("Accept: application/vnd.travis-ci.2+json"),
    ];
    get_json(sess, &url, &headers)
}

pub fn travis_post(sess: &Session, url: &str, token: &str) -> MyFuture<()> {
    let headers = vec![
        format!("Authorization: token {}", token),
        format!("Accept: application/vnd.travis-ci.2+json"),
    ];

    let response = post(sess, &append_url(TRAVIS_API_BASE, url), &headers);
    Box::new(response.map(|_| ()))
}

pub fn appveyor_get<T>(sess: &Session, url: &str, token: &str) -> MyFuture<T>
where
    T: Decodable + 'static,
{
    let headers = vec![
        format!("Authorization: Bearer {}", token),
        format!("Accept: application/json"),
    ];

    get_json(sess, &append_url(APPVEYOR_API_BASE, url), &headers)
}

pub fn appveyor_delete(sess: &Session, url: &str, token: &str) -> MyFuture<()> {
    let headers = vec![
        format!("Authorization: Bearer {}", token),
        format!("Accept: application/json"),
    ];

    let response = delete(sess, &append_url(APPVEYOR_API_BASE, url), &headers);
    Box::new(response.map(|_| ()))
}

pub fn azure_pipelines_get<T>(sess: &Session, url: &str, token: &str) -> MyFuture<T>
where
    T: Decodable + 'static,
{
    let base64 = base64::encode(&format!(":{}", token));
    let headers = vec![
        format!("Authorization: Basic {}", base64),
        format!("Accept: application/json"),
    ];

    get_json(sess, &append_url(AZURE_API_BASE, url), &headers)
}

pub fn azure_patch(sess: &Session, url: &str, token: &str, body: &str) -> MyFuture<()> {
    let base64 = base64::encode(&format!(":{}", token));
    let headers = vec![
        format!("Authorization: Basic {}", base64),
        format!("Accept: application/json"),
        format!("Content-Type: application/json"),
    ];

    let response = patch(sess, &append_url(AZURE_API_BASE, url), &headers, body);
    Box::new(response.map(|_| ()))
}

pub fn get_json<T>(sess: &Session, url: &str, headers: &[String]) -> MyFuture<T>
where
    T: Decodable + 'static,
{
    let response = get(sess, url, headers);
    let ret = response.and_then(|response| {
        let body = response.body.lock().unwrap();
        let json = try!(str::from_utf8(&body));
        let ret = try!(json::decode(json).chain_err(|| { format!("failed to decode: {}", json) }));
        Ok(ret)
    });
    Box::new(ret)
}

pub fn get(sess: &Session, url: &str, headers: &[String]) -> MyFuture<Response> {
    let mut handle = Easy::new();
    let mut list = List::new();
    t!(list.append(AGENT));
    for header in headers {
        t!(list.append(header));
    }

    t!(handle.http_headers(list));
    t!(handle.get(true));
    t!(handle.url(url));

    perform(sess, handle, url)
}

pub fn delete(sess: &Session, url: &str, headers: &[String]) -> MyFuture<Response> {
    let mut handle = Easy::new();
    let mut list = List::new();
    t!(list.append(AGENT));
    for header in headers {
        t!(list.append(header));
    }

    t!(handle.http_headers(list));
    t!(handle.custom_request("DELETE"));
    t!(handle.url(url));

    perform(sess, handle, url)
}

pub fn post(sess: &Session, url: &str, headers: &[String]) -> MyFuture<Response> {
    let mut handle = Easy::new();
    let mut list = List::new();
    t!(list.append(AGENT));
    for header in headers {
        t!(list.append(header));
    }

    t!(handle.http_headers(list));
    t!(handle.post(true));
    t!(handle.url(url));

    perform(sess, handle, url)
}

pub fn patch(sess: &Session, url: &str, headers: &[String], body: &str) -> MyFuture<Response> {
    let mut handle = Easy::new();
    let mut list = List::new();
    t!(list.append(AGENT));
    for header in headers {
        t!(list.append(header));
    }

    t!(handle.http_headers(list));
    t!(handle.post_fields_copy(body.as_bytes()));
    t!(handle.custom_request("PATCH"));
    t!(handle.url(url));

    perform(sess, handle, url)
}

pub fn perform(sess: &Session, mut easy: Easy, url: &str) -> MyFuture<Response> {
    println!("fetching: {}", url);
    let headers = Arc::new(Mutex::new(Vec::new()));
    let data = Arc::new(Mutex::new(Vec::new()));

    let (data2, headers2) = (data.clone(), headers.clone());
    t!(easy.header_function(move |data| {
        headers2.lock().unwrap().push(data.to_owned());
        true
    }));
    t!(easy.write_function(move |buf| {
        data2.lock().unwrap().extend_from_slice(&buf);
        Ok(buf.len())
    }));

    let response = sess.perform(easy);
    let url = url.to_string();
    let checked_response = response.map_err(|e| e.into()).and_then(move |mut easy| {
        println!("finished: {}", url);
        match t!(easy.response_code()) {
            200 | 204 => Ok(Response {
                easy: easy,
                headers: headers,
                body: data,
            }),
            code => Err(format!(
                "not a 200 code: {}\n\n{}\n",
                code,
                String::from_utf8_lossy(&data.lock().unwrap())
            )
            .into()),
        }
    });

    Box::new(checked_response)
}
