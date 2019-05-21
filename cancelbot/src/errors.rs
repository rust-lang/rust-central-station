#![allow(deprecated)]

use std::io;
use std::str;

use curl;
use rustc_serialize::json;
use tokio_curl;

error_chain! {
    types {
        BorsError, BorsErrorKind, BorsChainErr, BorsResult;
    }

    foreign_links {
        curl::Error, Curl;
        tokio_curl::PerformError, TokioCurl;
        json::DecoderError, Json;
        str::Utf8Error, NotUtf8;
        io::Error, Io;
    }
}
