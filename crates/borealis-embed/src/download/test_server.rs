//! Loopback HTTP/1.1 server for the download tests: serves byte blobs per path with `Range` support, can cut
//! the first response short, ignore `Range`, or answer a fixed status; records every request's headers.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

/// How one path answers.
#[derive(Clone, Default)]
pub struct Route {
    pub data: Vec<u8>,
    /// Ignore `Range` and always send the whole body with 200.
    pub ignore_range: bool,
    /// Close the connection after this many body bytes, on the first request to this path only.
    pub cut_first_after: Option<usize>,
    /// Answer this status with an empty body.
    pub status: Option<u16>,
    /// Claim this `Content-Length` on a 200 instead of the real one (body still the real data).
    pub claimed_length: Option<u64>,
}

/// One received request: path and lower-cased header names / values.
#[derive(Debug, Clone)]
pub struct Seen {
    pub path: String,
    pub headers: Vec<(String, String)>,
}

impl Seen {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }
}

pub struct TestServer {
    pub base: String,
    seen: Arc<Mutex<Vec<Seen>>>,
}

impl TestServer {
    pub fn start(routes: HashMap<String, Route>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let base = format!("http://{}", listener.local_addr().expect("addr"));
        let seen = Arc::new(Mutex::new(Vec::new()));
        let routes = Arc::new(routes);
        let cut_used: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>> = Arc::default();
        let log = Arc::clone(&seen);
        thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let (routes, log, cut_used) =
                    (Arc::clone(&routes), Arc::clone(&log), Arc::clone(&cut_used));
                thread::spawn(move || serve(stream, &routes, &log, &cut_used));
            }
        });
        Self { base, seen }
    }

    pub fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }

    pub fn requests(&self) -> Vec<Seen> {
        self.seen.lock().expect("log").clone()
    }
}

fn serve(
    stream: TcpStream,
    routes: &HashMap<String, Route>,
    log: &Mutex<Vec<Seen>>,
    cut_used: &Mutex<HashMap<String, Arc<AtomicBool>>>,
) {
    let mut reader = BufReader::new(stream.try_clone().expect("clone"));
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() {
        return;
    }
    let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
    let mut headers = Vec::new();
    loop {
        let mut h = String::new();
        if reader.read_line(&mut h).is_err() || h.trim().is_empty() {
            break;
        }
        if let Some((n, v)) = h.split_once(':') {
            headers.push((n.trim().to_ascii_lowercase(), v.trim().to_string()));
        }
    }
    let seen = Seen { path, headers };
    log.lock().expect("log").push(seen.clone());
    let mut out = stream;
    let Some(route) = routes.get(&seen.path) else {
        let _ = out
            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        return;
    };
    if let Some(status) = route.status {
        let _ = write!(
            out,
            "HTTP/1.1 {status} X\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        );
        return;
    }
    let total = route.data.len();
    let from = seen
        .header("range")
        .and_then(|r| r.strip_prefix("bytes="))
        .and_then(|r| r.strip_suffix('-'))
        .and_then(|n| n.parse::<usize>().ok())
        .filter(|_| !route.ignore_range);
    let (head, body) = match from {
        Some(start) if start >= total => (
            format!("HTTP/1.1 416 Range Not Satisfiable\r\nContent-Range: bytes */{total}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"),
            &route.data[..0],
        ),
        Some(start) => (
            format!(
                "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {start}-{}/{total}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                total - 1,
                total - start
            ),
            &route.data[start..],
        ),
        None => (
            format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                route.claimed_length.unwrap_or(total as u64)
            ),
            &route.data[..],
        ),
    };
    let _ = out.write_all(head.as_bytes());
    let first = {
        let mut used = cut_used.lock().expect("cut");
        let flag = used.entry(seen.path.clone()).or_default();
        !flag.swap(true, Ordering::SeqCst)
    };
    let body = match route.cut_first_after {
        Some(n) if first => &body[..n.min(body.len())],
        _ => body,
    };
    let _ = out.write_all(body);
    let _ = out.flush();
}
