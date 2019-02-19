use sha1::Sha1;
use std::path::{Path, PathBuf};
use std::error::Error;

static CACHE_PATH: &str = "/tmp/run-on-change";

fn cached_path(url: &str) -> PathBuf {
    Path::new(CACHE_PATH).join(Sha1::from(url).digest().to_string())
}

fn cached_hash(url: &str) -> Result<Option<String>, Box<Error>> {
    let path = cached_path(url);
    if path.exists() {
        Ok(Some(std::fs::read_to_string(&path)?.trim().into()))
    } else {
        Ok(None)
    }
}

fn fetch_url_hash(url: &str) -> Result<String, Box<Error>> {
    let mut hash = Sha1::new();
    let mut easy = curl::easy::Easy::new();
    easy.url(url)?;
    easy.useragent("rust-lang infra tooling (https://github.com/rust-lang/rust-central-station)")?;
    {
        let mut transfer = easy.transfer();
        transfer.write_function(|data| {
            hash.update(data);
            Ok(data.len())
        })?;
        transfer.perform()?;
    }
    if easy.response_code()? != 200 {
        Err(format!("request to {} returned status code {}", url, easy.response_code()?).into())
    } else {
        Ok(hash.digest().to_string())
    }
}

fn main() -> Result<(), Box<Error>> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() < 3 {
        eprintln!("usage: {} <url> <command ...>", args[0]);
        std::process::exit(1);
    }
    let url = &args[1];

    let url_hash = fetch_url_hash(url)?;
    if cached_hash(url)?.as_ref().map(|hash| hash.as_str()) != Some(&url_hash) {
        let status = std::process::Command::new(&args[2])
            .args(&args[3..])
            .status()?;

        if status.success() {
            let path = cached_path(url);
            if let Some(parent) = path.parent() {
                if !parent.exists() {
                    std::fs::create_dir_all(&parent)?;
                }
            }
            std::fs::write(&cached_path(url), format!("{}\n", url_hash).as_bytes())?;
        } else {
            std::process::exit(status.code().unwrap_or(1));
        }
    } else {
        eprintln!("content at {} didn't change, aborting", url);
    }

    Ok(())
}
