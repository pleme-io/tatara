//! The probe component: a WASI preview 2 command whose first argument picks
//! what it does, so one fixture exercises every bound the host enforces.
//!
//! - `serve <port>`: listen, answer `GET /healthz` with 200, `GET /crash`
//!   with 200 and then exit 3, `GET /connect/<host:port>` with whether a
//!   connection opened; log each request line to stdout.
//! - `exit <code>`, `spin`, `grow <mib>`, `connect <host:port>`,
//!   `bind <port>`, `read <path>`, `write <path>`: one action, then exit
//!   0 on success and 2 on refusal.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};

fn main() {
    println!("probe started");
    eprintln!("probe stderr");
    let args: Vec<String> = std::env::args().skip(1).collect();
    let arg = |i: usize| args.get(i).cloned().unwrap_or_default();
    let code = match arg(0).as_str() {
        "serve" => serve(arg(1).parse().unwrap_or(8080)),
        "exit" => arg(1).parse().unwrap_or(0),
        "spin" => spin(),
        "grow" => grow(arg(1).parse().unwrap_or(64)),
        "connect" => report(TcpStream::connect(arg(1)).map(drop)),
        "bind" => report(TcpListener::bind(("0.0.0.0", arg(1).parse().unwrap_or(0))).map(drop)),
        "read" => report(std::fs::read_to_string(arg(1)).map(|s| print!("{s}"))),
        "write" => report(std::fs::write(arg(1), b"probe")),
        _ => 0,
    };
    std::process::exit(code);
}

fn report(result: std::io::Result<()>) -> i32 {
    match result {
        Ok(()) => {
            println!("ok");
            0
        }
        Err(e) => {
            println!("refused: {e}");
            2
        }
    }
}

fn spin() -> i32 {
    let mut n: u64 = 0;
    loop {
        n = std::hint::black_box(n.wrapping_add(1));
    }
}

fn grow(mib: usize) -> i32 {
    let mut held: Vec<Vec<u8>> = Vec::new();
    for _ in 0..mib {
        held.push(vec![1u8; 1 << 20]);
    }
    println!("grew {}", held.len());
    0
}

fn serve(port: u16) -> i32 {
    let listener = match TcpListener::bind(("0.0.0.0", port)) {
        Ok(l) => l,
        Err(e) => {
            println!("refused: {e}");
            return 2;
        }
    };
    println!("listening on {port}");
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        let mut line = String::new();
        let mut reader = BufReader::new(&mut stream);
        if reader.read_line(&mut line).is_err() {
            continue;
        }
        let mut header = String::new();
        while reader.read_line(&mut header).map(|n| n > 2).unwrap_or(false) {
            header.clear();
        }
        let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
        println!("request {path}");
        let (status, body) = if path == "/healthz" || path == "/crash" {
            ("200 OK", "ok\n".to_string())
        } else if let Some(target) = path.strip_prefix("/connect/") {
            match TcpStream::connect(target) {
                Ok(mut s) => {
                    let _ = s.write_all(b"GET / HTTP/1.0\r\n\r\n");
                    let mut answer = String::new();
                    let _ = s.read_to_string(&mut answer);
                    ("200 OK", format!("connected\n{answer}"))
                }
                Err(e) => ("502 Bad Gateway", format!("refused: {e}\n")),
            }
        } else {
            ("404 Not Found", "no\n".to_string())
        };
        let _ = write!(
            stream,
            "HTTP/1.1 {status}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = stream.flush();
        drop(stream);
        if path == "/crash" {
            eprintln!("crashing on request");
            std::process::exit(3);
        }
    }
    0
}
