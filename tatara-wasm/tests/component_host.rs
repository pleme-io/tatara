//! End to end: WASI preview 2 components spawned long-running through
//! `WasmEngine::spawn` on wasmtime, against the prebuilt fixtures. Each test
//! pins one bound the host enforces, with the differential that proves the
//! bound and not a broken guest is what refused.
//!
//! The probe fixture is built with Rust's std, whose `exit(n)` goes through
//! `wasi:cli/exit#exit` and carries only success or failure, so any non-zero
//! `n` reads as code 1; the refusals are told apart by what the probe
//! printed. A guest that calls `exit-with-code` reports its code whole.

#![cfg(all(feature = "wasi-http", feature = "fixtures"))]

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tatara_wasm::{
    engine_for, fixtures, GuestStream, NetworkGrant, OutputSink, Peer, Preopen, WasiGrant,
    WasiPreview, WasmBoot, WasmCapabilities, WasmEngineError, WasmExit, WasmFeatures, WasmLimits,
    WasmModuleSource, WasmProcess, WasmRuntime, WasmSpawn,
};

#[derive(Default)]
struct Captured(Mutex<Vec<(GuestStream, Vec<u8>)>>, Mutex<bool>);

impl OutputSink for Captured {
    fn write(&self, stream: GuestStream, bytes: &[u8]) {
        self.0.lock().unwrap().push((stream, bytes.to_vec()));
    }

    fn finish(&self) {
        *self.1.lock().unwrap() = true;
    }
}

impl Captured {
    fn text(&self, stream: GuestStream) -> String {
        let chunks = self.0.lock().unwrap();
        chunks
            .iter()
            .filter(|(s, _)| *s == stream)
            .map(|(_, b)| String::from_utf8_lossy(b).into_owned())
            .collect()
    }
}

fn grant(argv: &[&str], network: NetworkGrant, preopens: Vec<Preopen>) -> WasmCapabilities {
    let mut full = vec!["probe".to_string()];
    full.extend(argv.iter().map(|a| (*a).to_string()));
    WasmCapabilities {
        wasi: Some(WasiGrant {
            stdout: true,
            stderr: true,
            argv: full,
            preopens,
            network,
            ..WasiGrant::default()
        }),
        ..WasmCapabilities::default()
    }
}

fn spawn_of(component: &[u8], capabilities: WasmCapabilities, limits: WasmLimits) -> WasmSpawn {
    WasmSpawn {
        boot: WasmBoot {
            module: WasmModuleSource::Bytes(component.to_vec()),
            runtime: WasmRuntime::Wasmtime,
            preview: WasiPreview::P2,
            features: WasmFeatures::default(),
            capabilities,
            name: "probe".into(),
        },
        limits,
        serve_http: None,
    }
}

fn start(spawn: &WasmSpawn) -> (WasmProcess, Arc<Captured>) {
    let sink = Arc::new(Captured::default());
    let engine = engine_for(WasmRuntime::Wasmtime).expect("wasmtime compiled in");
    let process = engine
        .spawn(spawn, Arc::clone(&sink) as Arc<dyn OutputSink>)
        .expect("spawn");
    (process, sink)
}

async fn ended(process: &mut WasmProcess) -> WasmExit {
    tokio::time::timeout(Duration::from_secs(30), process.wait())
        .await
        .expect("the guest ended within 30s")
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .expect("a free port")
}

fn get(port: u16, path: &str) -> std::io::Result<String> {
    let mut s = TcpStream::connect(("127.0.0.1", port))?;
    s.set_read_timeout(Some(Duration::from_secs(10)))?;
    write!(s, "GET {path} HTTP/1.1\r\nhost: localhost\r\nconnection: close\r\n\r\n")?;
    let mut out = String::new();
    s.read_to_string(&mut out)?;
    Ok(out)
}

async fn get_when_up(port: u16, path: &str) -> String {
    for _ in 0..200 {
        let attempt = tokio::task::spawn_blocking({
            let path = path.to_string();
            move || get(port, &path)
        })
        .await
        .expect("join");
        if let Ok(answer) = attempt {
            return answer;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("nothing answered on port {port}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_command_streams_both_outputs_and_reports_its_exit_code() {
    let (mut p, sink) = start(&spawn_of(
        fixtures::PROBE,
        grant(&["exit", "4"], NetworkGrant::default(), vec![]),
        WasmLimits::default(),
    ));
    assert_eq!(ended(&mut p).await, WasmExit::Code { code: 1 });
    assert_eq!(sink.text(GuestStream::Stdout), "probe started\n");
    assert_eq!(sink.text(GuestStream::Stderr), "probe stderr\n");
    assert!(*sink.1.lock().unwrap(), "the sink was never told the guest ended");
}

/// RED RUN (2026-10-27): the stop branch removed from `run_command`'s select
/// → `the guest ended within 30s` panics: a spinning guest outlives its stop.
#[tokio::test(flavor = "multi_thread")]
async fn a_spinning_guest_is_stopped() {
    let (mut p, _) = start(&spawn_of(
        fixtures::PROBE,
        grant(&["spin"], NetworkGrant::default(), vec![]),
        WasmLimits::default(),
    ));
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(p.exit(), None, "a spinning guest exited by itself");
    p.stop();
    assert_eq!(ended(&mut p).await, WasmExit::Stopped);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_spinning_guest_hits_its_deadline() {
    let (mut p, _) = start(&spawn_of(
        fixtures::PROBE,
        grant(&["spin"], NetworkGrant::default(), vec![]),
        WasmLimits {
            deadline: Some(Duration::from_millis(300)),
            ..WasmLimits::default()
        },
    ));
    assert_eq!(
        ended(&mut p).await,
        WasmExit::Deadline {
            after: Duration::from_millis(300)
        }
    );
}

/// RED RUN (2026-10-27): `store.limiter` not installed in `Recipe::store` →
/// `Code { code: 0 }` (the guest grew 64 MiB under a 16 MiB limit).
#[tokio::test(flavor = "multi_thread")]
async fn growth_past_the_memory_limit_ends_the_guest() {
    let limits = WasmLimits {
        memory_bytes: Some(16 << 20),
        ..WasmLimits::default()
    };
    let (mut p, _) = start(&spawn_of(
        fixtures::PROBE,
        grant(&["grow", "64"], NetworkGrant::default(), vec![]),
        limits,
    ));
    assert_eq!(
        ended(&mut p).await,
        WasmExit::MemoryLimit {
            limit_bytes: 16 << 20
        }
    );
    let (mut fits, _) = start(&spawn_of(
        fixtures::PROBE,
        grant(&["grow", "4"], NetworkGrant::default(), vec![]),
        limits,
    ));
    assert_eq!(ended(&mut fits).await, WasmExit::Code { code: 0 });
}

/// The refused case declares a different peer, so TCP is on and only the
/// address check stands between the guest and the target.
///
/// RED RUN (2026-10-27): the address check answering `true` → the undeclared
/// connect exits 0. (The first draft refused with an empty grant and stayed
/// green under that mutation: an empty grant turns TCP off before any check.)
#[tokio::test(flavor = "multi_thread")]
async fn a_connect_reaches_only_a_declared_peer() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let target = format!("127.0.0.1:{port}");

    let declared = NetworkGrant {
        connect: vec![Peer::new("127.0.0.1", port)],
        ..NetworkGrant::default()
    };
    let (mut ok, sink) = start(&spawn_of(
        fixtures::PROBE,
        grant(&["connect", &target], declared, vec![]),
        WasmLimits::default(),
    ));
    assert_eq!(ended(&mut ok).await, WasmExit::Code { code: 0 }, "{}", sink.text(GuestStream::Stdout));

    let elsewhere = NetworkGrant {
        connect: vec![Peer::new("127.0.0.1", free_port())],
        ..NetworkGrant::default()
    };
    let (mut refused, said) = start(&spawn_of(
        fixtures::PROBE,
        grant(&["connect", &target], elsewhere, vec![]),
        WasmLimits::default(),
    ));
    assert_eq!(ended(&mut refused).await, WasmExit::Code { code: 1 });
    assert!(said.text(GuestStream::Stdout).contains("refused:"));
    drop(listener);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_bind_succeeds_only_on_a_declared_port() {
    let port = free_port();
    let open = NetworkGrant {
        bind: vec![port],
        ..NetworkGrant::default()
    };
    let (mut ok, _) = start(&spawn_of(
        fixtures::PROBE,
        grant(&["bind", &port.to_string()], open, vec![]),
        WasmLimits::default(),
    ));
    assert_eq!(ended(&mut ok).await, WasmExit::Code { code: 0 });
    let elsewhere = NetworkGrant {
        bind: vec![free_port()],
        ..NetworkGrant::default()
    };
    let (mut refused, said) = start(&spawn_of(
        fixtures::PROBE,
        grant(&["bind", &port.to_string()], elsewhere, vec![]),
        WasmLimits::default(),
    ));
    assert_eq!(ended(&mut refused).await, WasmExit::Code { code: 1 });
    assert!(said.text(GuestStream::Stdout).contains("refused:"));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_filesystem_is_only_what_is_preopened() {
    let dir = std::env::temp_dir().join(format!("tatara-wasm-fs-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("dir");
    std::fs::write(dir.join("hello"), "from the host\n").expect("seed");
    let read_only = vec![Preopen {
        host_path: dir.clone(),
        guest_path: "/data".into(),
        writable: false,
    }];

    let (mut read, sink) = start(&spawn_of(
        fixtures::PROBE,
        grant(&["read", "/data/hello"], NetworkGrant::default(), read_only.clone()),
        WasmLimits::default(),
    ));
    assert_eq!(ended(&mut read).await, WasmExit::Code { code: 0 });
    assert!(sink.text(GuestStream::Stdout).contains("from the host"));

    let (mut write, said) = start(&spawn_of(
        fixtures::PROBE,
        grant(&["write", "/data/out"], NetworkGrant::default(), read_only),
        WasmLimits::default(),
    ));
    assert_eq!(ended(&mut write).await, WasmExit::Code { code: 1 }, "a read-only preopen was written");
    assert!(said.text(GuestStream::Stdout).contains("refused:"));
    assert!(!dir.join("out").exists());

    let (mut none, said) = start(&spawn_of(
        fixtures::PROBE,
        grant(&["read", "/data/hello"], NetworkGrant::default(), vec![]),
        WasmLimits::default(),
    ));
    assert_eq!(ended(&mut none).await, WasmExit::Code { code: 1 });
    assert!(said.text(GuestStream::Stdout).contains("refused:"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_command_serves_http_on_its_declared_port_until_it_crashes() {
    let port = free_port();
    let open = NetworkGrant {
        bind: vec![port],
        ..NetworkGrant::default()
    };
    let (mut p, sink) = start(&spawn_of(
        fixtures::PROBE,
        grant(&["serve", &port.to_string()], open, vec![]),
        WasmLimits::default(),
    ));
    assert!(get_when_up(port, "/healthz").await.starts_with("HTTP/1.1 200"));
    let _ = get_when_up(port, "/crash").await;
    assert_eq!(ended(&mut p).await, WasmExit::Code { code: 1 });
    assert!(sink.text(GuestStream::Stdout).contains("request /healthz"));
    assert!(sink.text(GuestStream::Stderr).contains("crashing on request"));
}

/// RED RUN (2026-10-27): `send_request` without the `names_peer` check →
/// the undeclared fetch is attempted and fails as a refused connection, not
/// as `HttpRequestDenied`.
#[tokio::test(flavor = "multi_thread")]
async fn a_proxy_component_is_served_and_its_outgoing_requests_are_bounded() {
    let upstream = free_port();
    let up = NetworkGrant {
        bind: vec![upstream],
        ..NetworkGrant::default()
    };
    let (mut origin, _) = start(&spawn_of(
        fixtures::PROBE,
        grant(&["serve", &upstream.to_string()], up, vec![]),
        WasmLimits::default(),
    ));
    let _ = get_when_up(upstream, "/healthz").await;

    let port = free_port();
    let net = NetworkGrant {
        bind: vec![port],
        connect: vec![Peer::new("127.0.0.1", upstream)],
    };
    let mut spawn = spawn_of(fixtures::GREETER, grant(&[], net, vec![]), WasmLimits::default());
    spawn.serve_http = Some(port);
    let (mut proxy, sink) = start(&spawn);

    let hello = get_when_up(port, "/hi").await;
    assert!(hello.starts_with("HTTP/1.1 200"), "{hello}");
    assert!(hello.contains("hello from /hi"), "{hello}");

    let fetched = get_when_up(port, &format!("/fetch/127.0.0.1:{upstream}")).await;
    assert!(fetched.contains("fetched 404"), "the origin answers / with 404: {fetched}");

    let denied = get_when_up(port, &format!("/fetch/127.0.0.1:{}", free_port())).await;
    assert!(denied.starts_with("HTTP/1.1 502"), "{denied}");
    assert!(denied.contains("HttpRequestDenied"), "{denied}");

    assert!(sink.text(GuestStream::Stdout).contains("greeter /hi"));
    proxy.stop();
    assert_eq!(ended(&mut proxy).await, WasmExit::Stopped);
    origin.stop();
    assert_eq!(ended(&mut origin).await, WasmExit::Stopped);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_proxy_port_outside_the_grant_is_refused_before_anything_runs() {
    let mut spawn = spawn_of(
        fixtures::GREETER,
        grant(&[], NetworkGrant::default(), vec![]),
        WasmLimits::default(),
    );
    spawn.serve_http = Some(free_port());
    let engine = engine_for(WasmRuntime::Wasmtime).expect("wasmtime");
    let err = engine
        .spawn(&spawn, Arc::new(Captured::default()))
        .expect_err("served a port the grant does not bind");
    assert!(matches!(err, WasmEngineError::ServePortNotGranted(_)));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_component_with_no_wasi_grant_is_refused_by_import_name() {
    let spawn = spawn_of(fixtures::PROBE, WasmCapabilities::closed(), WasmLimits::default());
    let engine = engine_for(WasmRuntime::Wasmtime).expect("wasmtime");
    let err = engine
        .spawn(&spawn, Arc::new(Captured::default()))
        .expect_err("a WASI component spawned with no WASI grant");
    match err {
        WasmEngineError::CapabilityNotGranted { module, .. } => assert!(module.starts_with("wasi:")),
        other => panic!("wrong error: {other:?}"),
    }
}

fn rounds(text: &str) -> u64 {
    text.lines()
        .find_map(|l| l.strip_prefix("rounds "))
        .and_then(|n| n.trim().parse().ok())
        .expect("the probe printed its rounds")
}

/// RED RUN (2026-10-28): `throttle_pause` always `None` → the 100-millicore
/// guest ran as many rounds as the whole-core one.
#[tokio::test(flavor = "multi_thread")]
async fn a_cpu_limit_holds_the_guest_to_its_share() {
    let run = |cpu_millis| async move {
        let limits = WasmLimits {
            cpu_millis,
            ..WasmLimits::default()
        };
        let (mut p, sink) = start(&spawn_of(
            fixtures::PROBE,
            grant(&["count", "600"], NetworkGrant::default(), vec![]),
            limits,
        ));
        assert_eq!(ended(&mut p).await, WasmExit::Code { code: 0 });
        rounds(&sink.text(GuestStream::Stdout))
    };
    let whole = run(None).await;
    let tenth = run(Some(100)).await;
    assert!(
        tenth * 3 < whole,
        "a 100-millicore guest ran {tenth} rounds against {whole} on a whole core"
    );
}

/// A command that calls the unstable `wasi:cli/exit#exit-with-code` with 7.
/// Rust's std never calls it (it exits through `exit(err)`, read as 1), so
/// the guest is written in the component text format.
const EXIT_SEVEN: &str = r#"
(component
  (import "wasi:cli/exit@0.2.6" (instance $exit
    (export "exit-with-code" (func (param "status-code" u8)))))
  (core func $exit_with_code (canon lower (func $exit "exit-with-code")))
  (core module $m
    (import "host" "exit-with-code" (func $e (param i32)))
    (func (export "run") (result i32)
      (call $e (i32.const 7))
      (i32.const 0)))
  (core instance $i (instantiate $m
    (with "host" (instance (export "exit-with-code" (func $exit_with_code))))))
  (func $run (result (result)) (canon lift (core func $i "run")))
  (instance $runner (export "run" (func $run)))
  (export "wasi:cli/run@0.2.6" (instance $runner)))
"#;

/// RED RUN (2026-10-28): the host linked without `cli_exit_with_code` →
/// the spawn is refused: `wasi:cli/exit@0.2.6` has no `exit-with-code`.
#[tokio::test(flavor = "multi_thread")]
async fn a_guest_exit_code_is_reported_whole_through_exit_with_code() {
    let mut spawn = spawn_of(&[], grant(&[], NetworkGrant::default(), vec![]), WasmLimits::default());
    spawn.boot.module = WasmModuleSource::Wat(EXIT_SEVEN.to_string());
    let (mut p, _) = start(&spawn);
    assert_eq!(ended(&mut p).await, WasmExit::Code { code: 7 });
}

#[test]
fn spawn_outside_a_runtime_is_a_typed_refusal() {
    let spawn = spawn_of(
        fixtures::PROBE,
        grant(&["exit", "0"], NetworkGrant::default(), vec![]),
        WasmLimits::default(),
    );
    let engine = engine_for(WasmRuntime::Wasmtime).expect("wasmtime");
    let err = engine
        .spawn(&spawn, Arc::new(Captured::default()))
        .expect_err("spawned with no runtime to run on");
    assert!(matches!(err, WasmEngineError::NoAsyncRuntime));
}
