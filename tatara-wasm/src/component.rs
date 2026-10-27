//! The wasmtime component host: WASI preview 2 components, long-running.
//!
//! [`ComponentHost::spawn`] compiles a component, links exactly what its
//! [`WasmCapabilities`] grant, and runs it on the caller's tokio runtime:
//!
//! - a component exporting `wasi:cli/run` runs once, to its exit;
//! - a component exporting `wasi:http/incoming-handler` (with the
//!   `wasi-http` feature) is served on [`WasmSpawn::serve_http`], one
//!   instance per request, until it is stopped.
//!
//! Every bound is enforced by the engine, not asked of the guest: memory by a
//! store limiter that traps the growth past [`WasmLimits::memory_bytes`],
//! time by epoch interruption (a guest spinning in pure compute still yields
//! every tick, so a deadline or a stop always lands), and the network by the
//! socket address check and the outgoing-HTTP authority check, both read off
//! the grant's [`NetworkGrant`].

#![cfg(feature = "runtime-wasmtime")]

use std::net::{IpAddr, SocketAddr};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use tokio::io::AsyncWrite;
use tokio::sync::watch;
use wasmtime::component::{Component, Linker, ResourceTable};
use wasmtime::{Config, Engine, ResourceLimiter, Store};
use wasmtime_wasi::cli::{IsTerminal, StdoutStream};
use wasmtime_wasi::p2::bindings::CommandPre;
use wasmtime_wasi::{
    DirPerms, FilePerms, I32Exit, SocketAddrUse, WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView,
};

use crate::engine::{
    check_component_imports, read_module, GuestStream, NetworkGrant, OutputSink, WasiGrant,
    WasmCapabilities, WasmEngineError, WasmExit, WasmLimits, WasmProcess, WasmSpawn,
};

/// How often the engine's epoch advances. A running guest yields to the host
/// at least this often, which bounds how late a stop or a deadline lands.
pub const EPOCH_TICK: Duration = Duration::from_millis(10);

const HTTP_LINKED: bool = cfg!(feature = "wasi-http");

/// The async, epoch-interrupted wasmtime engine components run on.
pub struct ComponentHost {
    engine: Engine,
}

impl ComponentHost {
    /// Build the engine and start its epoch ticker. The ticker holds only a
    /// weak handle, so it ends when the engine is dropped.
    ///
    /// # Errors
    /// [`WasmEngineError::Instantiate`] if the engine or its ticker cannot
    /// be built.
    pub fn new() -> Result<Self, WasmEngineError> {
        let mut config = Config::new();
        config.async_support(true);
        config.wasm_component_model(true);
        config.epoch_interruption(true);
        let engine = Engine::new(&config).map_err(instantiate)?;
        let weak = engine.weak();
        std::thread::Builder::new()
            .name("tatara-wasm-epoch".to_string())
            .spawn(move || {
                while let Some(engine) = weak.upgrade() {
                    engine.increment_epoch();
                    drop(engine);
                    std::thread::sleep(EPOCH_TICK);
                }
            })
            .map_err(|e| WasmEngineError::Instantiate(e.to_string()))?;
        Ok(Self { engine })
    }

    /// Start `spawn`'s component and return at once.
    ///
    /// # Errors
    /// [`WasmEngineError::NoAsyncRuntime`] outside a tokio runtime; a compile,
    /// import-grant or instantiate refusal; [`WasmEngineError::NoEntry`] for a
    /// component with neither entry export; for a proxy component,
    /// [`WasmEngineError::ServePortMissing`] or a bind failure.
    pub fn spawn(
        &self,
        spawn: &WasmSpawn,
        sink: Arc<dyn OutputSink>,
    ) -> Result<WasmProcess, WasmEngineError> {
        let runtime = tokio::runtime::Handle::try_current()
            .map_err(|_| WasmEngineError::NoAsyncRuntime)?;
        let bytes = read_module(&spawn.boot.module)?;
        let component = Component::new(&self.engine, &bytes)
            .map_err(|e| WasmEngineError::Compile(format!("{e:#}")))?;
        let ty = component.component_type();
        check_component_imports(
            &spawn.boot.capabilities,
            HTTP_LINKED,
            ty.imports(&self.engine).map(|(name, _)| name),
        )?;
        let world = World::of(ty.exports(&self.engine).map(|(name, _)| name))?;

        let mut linker = Linker::<Guest>::new(&self.engine);
        if spawn.boot.capabilities.wasi.is_some() {
            wasmtime_wasi::p2::add_to_linker_async(&mut linker).map_err(instantiate)?;
            #[cfg(feature = "wasi-http")]
            wasmtime_wasi_http::add_only_http_to_linker_async(&mut linker)
                .map_err(instantiate)?;
        }
        let pre = linker.instantiate_pre(&component).map_err(instantiate)?;

        let (exit_tx, exit_rx) = watch::channel(None);
        let (stop_tx, stop_rx) = watch::channel(false);
        let recipe = Arc::new(Recipe {
            capabilities: spawn.boot.capabilities.clone(),
            limits: spawn.limits,
            sink,
        });
        match world {
            World::Command => {
                let pre = CommandPre::new(pre).map_err(instantiate)?;
                runtime.spawn(run_command(pre, recipe, stop_rx, exit_tx));
            }
            #[cfg(feature = "wasi-http")]
            World::Proxy => {
                proxy::start(&runtime, pre, recipe, spawn.serve_http, stop_rx, exit_tx)?;
            }
        }
        Ok(WasmProcess::new(spawn.boot.name.clone(), exit_rx, stop_tx))
    }
}

fn instantiate(e: impl std::fmt::Display) -> WasmEngineError {
    WasmEngineError::Instantiate(format!("{e:#}"))
}

/// Which entry a component offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum World {
    /// `wasi:cli/run`.
    Command,
    /// `wasi:http/incoming-handler`.
    #[cfg(feature = "wasi-http")]
    Proxy,
}

impl World {
    fn of<'a>(exports: impl Iterator<Item = &'a str>) -> Result<Self, WasmEngineError> {
        let names: Vec<String> = exports.map(str::to_string).collect();
        if names.iter().any(|n| n.starts_with("wasi:cli/run@")) {
            return Ok(Self::Command);
        }
        #[cfg(feature = "wasi-http")]
        if names
            .iter()
            .any(|n| n.starts_with("wasi:http/incoming-handler@"))
        {
            return Ok(Self::Proxy);
        }
        Err(WasmEngineError::NoEntry { exports: names })
    }
}

/// Everything needed to build a fresh store for one run or one request.
struct Recipe {
    capabilities: WasmCapabilities,
    limits: WasmLimits,
    sink: Arc<dyn OutputSink>,
}

impl Recipe {
    fn store(&self, engine: &Engine) -> Result<Store<Guest>, WasmEngineError> {
        let grant = self.capabilities.wasi.as_ref();
        let network = Arc::new(grant.map(|g| g.network.clone()).unwrap_or_default());
        let guest = Guest {
            wasi: wasi_ctx(grant, &network, &self.sink)?,
            table: ResourceTable::new(),
            #[cfg(feature = "wasi-http")]
            http: wasmtime_wasi_http::WasiHttpCtx::new(),
            memory: Ceiling::new(self.limits.memory_bytes),
            network,
        };
        let mut store = Store::new(engine, guest);
        store.limiter(|g| &mut g.memory);
        store.set_epoch_deadline(1);
        store.epoch_deadline_async_yield_and_update(1);
        Ok(store)
    }
}

/// One store's state.
struct Guest {
    wasi: WasiCtx,
    table: ResourceTable,
    #[cfg(feature = "wasi-http")]
    http: wasmtime_wasi_http::WasiHttpCtx,
    memory: Ceiling,
    #[cfg_attr(not(feature = "wasi-http"), allow(dead_code))]
    network: Arc<NetworkGrant>,
}

impl WasiView for Guest {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.table,
        }
    }
}

/// Build the WASI context a grant describes. With no grant the context is
/// empty and, since no WASI is linked, unreachable anyway.
fn wasi_ctx(
    grant: Option<&WasiGrant>,
    network: &Arc<NetworkGrant>,
    sink: &Arc<dyn OutputSink>,
) -> Result<WasiCtx, WasmEngineError> {
    let mut builder = WasiCtxBuilder::new();
    if let Some(g) = grant {
        if g.stdout {
            builder.stdout(SinkStream {
                stream: GuestStream::Stdout,
                sink: Arc::clone(sink),
            });
        }
        if g.stderr {
            builder.stderr(SinkStream {
                stream: GuestStream::Stderr,
                sink: Arc::clone(sink),
            });
        }
        apply_grant(&mut builder, g)?;
    }
    let open = !network.is_closed();
    builder.allow_tcp(open);
    builder.allow_udp(open);
    builder.allow_ip_name_lookup(!network.connect.is_empty());
    let checked = Arc::clone(network);
    builder.socket_addr_check(move |addr, use_| {
        let net = Arc::clone(&checked);
        Box::pin(async move { admits_socket(&net, addr, use_).await })
    });
    Ok(builder.build())
}

/// Apply the parts of a grant every WASI context shares: environment,
/// arguments and preopened directories. Stdio is wired by the caller, since
/// a run-to-completion guest captures it and a spawned one streams it.
///
/// # Errors
/// [`WasmEngineError::Io`] if a preopened directory cannot be opened.
pub(crate) fn apply_grant(
    builder: &mut WasiCtxBuilder,
    grant: &WasiGrant,
) -> Result<(), WasmEngineError> {
    for (k, v) in &grant.env {
        builder.env(k, v);
    }
    if !grant.argv.is_empty() {
        builder.args(&grant.argv);
    }
    for p in &grant.preopens {
        let (dir_perms, file_perms) = if p.writable {
            (DirPerms::all(), FilePerms::all())
        } else {
            (DirPerms::READ, FilePerms::READ)
        };
        builder
            .preopened_dir(&p.host_path, &p.guest_path, dir_perms, file_perms)
            .map_err(|e| WasmEngineError::Io(format!("preopen {}: {e}", p.host_path.display())))?;
    }
    Ok(())
}

/// Whether `net` admits a guest's use of `addr`.
///
/// A bind is admitted on a declared port (or an ephemeral one, once a peer
/// is declared). A connect or datagram is admitted only to a declared peer's
/// port, at an address that peer's host is or resolves to.
pub async fn admits_socket(net: &NetworkGrant, addr: SocketAddr, use_: SocketAddrUse) -> bool {
    match use_ {
        SocketAddrUse::TcpBind | SocketAddrUse::UdpBind => net.admits_bind(addr.port()),
        SocketAddrUse::TcpConnect
        | SocketAddrUse::UdpConnect
        | SocketAddrUse::UdpOutgoingDatagram => {
            let want = canonical(addr.ip());
            for peer in net.connect.iter().filter(|p| p.port == addr.port()) {
                if let Ok(ip) = peer.host.parse::<IpAddr>() {
                    if canonical(ip) == want {
                        return true;
                    }
                    continue;
                }
                if let Ok(found) = tokio::net::lookup_host((peer.host.as_str(), peer.port)).await {
                    if found.into_iter().any(|a| canonical(a.ip()) == want) {
                        return true;
                    }
                }
            }
            false
        }
    }
}

fn canonical(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(ip, IpAddr::V4),
        IpAddr::V4(_) => ip,
    }
}

/// A guest output stream that hands each write to the sink.
#[derive(Clone)]
struct SinkStream {
    stream: GuestStream,
    sink: Arc<dyn OutputSink>,
}

impl IsTerminal for SinkStream {
    fn is_terminal(&self) -> bool {
        false
    }
}

impl StdoutStream for SinkStream {
    fn async_stream(&self) -> Box<dyn AsyncWrite + Send + Sync> {
        Box::new(self.clone())
    }
}

impl AsyncWrite for SinkStream {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        self.sink.write(self.stream, buf);
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

/// The store-wide memory ceiling: the sum of every linear memory's size.
struct Ceiling {
    limit: Option<usize>,
    held: usize,
}

impl Ceiling {
    fn new(limit: Option<u64>) -> Self {
        Self {
            limit: limit.map(|l| usize::try_from(l).unwrap_or(usize::MAX)),
            held: 0,
        }
    }
}

/// The trap a growth past the ceiling raises.
#[derive(Debug, thiserror::Error)]
#[error("linear memory would pass the {limit}-byte limit")]
struct CeilingHit {
    limit: usize,
}

impl ResourceLimiter for Ceiling {
    fn memory_growing(
        &mut self,
        current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> anyhow::Result<bool> {
        let held = self.held.saturating_add(desired.saturating_sub(current));
        if let Some(limit) = self.limit {
            if held > limit {
                return Err(anyhow::Error::new(CeilingHit { limit }));
            }
        }
        self.held = held;
        Ok(true)
    }

    fn table_growing(
        &mut self,
        _current: usize,
        _desired: usize,
        _maximum: Option<usize>,
    ) -> anyhow::Result<bool> {
        Ok(true)
    }
}

/// Read a run's result into an exit.
fn classify(result: anyhow::Result<Result<(), ()>>) -> WasmExit {
    match result {
        Ok(Ok(())) => WasmExit::Code { code: 0 },
        Ok(Err(())) => WasmExit::Code { code: 1 },
        Err(e) => {
            if let Some(exit) = found::<I32Exit>(&e) {
                return WasmExit::Code { code: exit.0 };
            }
            if let Some(hit) = found::<CeilingHit>(&e) {
                return WasmExit::MemoryLimit {
                    limit_bytes: u64::try_from(hit.limit).unwrap_or(u64::MAX),
                };
            }
            WasmExit::Trapped {
                message: format!("{e:#}"),
            }
        }
    }
}

fn found<T: std::error::Error + Send + Sync + 'static>(e: &anyhow::Error) -> Option<&T> {
    e.downcast_ref::<T>()
        .or_else(|| e.chain().find_map(|c| c.downcast_ref::<T>()))
}

/// Resolves when `deadline` has passed; never, with no deadline.
async fn elapsed(deadline: Option<Duration>) -> Duration {
    match deadline {
        Some(after) => {
            tokio::time::sleep(after).await;
            after
        }
        None => std::future::pending().await,
    }
}

/// Resolves once a stop is asked for. A dropped handle is not a stop.
async fn stopped(mut stop: watch::Receiver<bool>) {
    if stop.wait_for(|s| *s).await.is_err() {
        std::future::pending::<()>().await;
    }
}

async fn run_command(
    pre: CommandPre<Guest>,
    recipe: Arc<Recipe>,
    stop: watch::Receiver<bool>,
    exit: watch::Sender<Option<WasmExit>>,
) {
    let outcome = match recipe.store(pre.engine()) {
        Err(e) => WasmExit::Trapped {
            message: e.to_string(),
        },
        Ok(mut store) => {
            let run = async {
                let command = pre.instantiate_async(&mut store).await?;
                command.wasi_cli_run().call_run(&mut store).await
            };
            tokio::select! {
                result = run => classify(result),
                after = elapsed(recipe.limits.deadline) => WasmExit::Deadline { after },
                () = stopped(stop) => WasmExit::Stopped,
            }
        }
    };
    recipe.sink.finish();
    exit.send_replace(Some(outcome));
}

#[cfg(feature = "wasi-http")]
mod proxy {
    use std::convert::Infallible;
    use std::net::Ipv4Addr;
    use std::sync::Arc;

    use http_body_util::BodyExt;
    use hyper::body::Bytes;
    use tokio::sync::watch;
    use wasmtime::component::{InstancePre, ResourceTable};
    use wasmtime_wasi_http::bindings::http::types::{ErrorCode, Scheme};
    use wasmtime_wasi_http::bindings::ProxyPre;
    use wasmtime_wasi_http::body::HyperOutgoingBody;
    use wasmtime_wasi_http::io::TokioIo;
    use wasmtime_wasi_http::types::{
        default_send_request, HostFutureIncomingResponse, OutgoingRequestConfig,
    };
    use wasmtime_wasi_http::{HttpResult, WasiHttpCtx, WasiHttpView};

    use super::{classify, elapsed, instantiate, stopped, Guest, Recipe};
    use crate::engine::{GuestStream, WasmEngineError, WasmExit};

    impl WasiHttpView for Guest {
        fn ctx(&mut self) -> &mut WasiHttpCtx {
            &mut self.http
        }

        fn table(&mut self) -> &mut ResourceTable {
            &mut self.table
        }

        fn send_request(
            &mut self,
            request: hyper::Request<HyperOutgoingBody>,
            config: OutgoingRequestConfig,
        ) -> HttpResult<HostFutureIncomingResponse> {
            let authority = request.uri().authority();
            let host = authority.map_or("", |a| a.host());
            let port = authority
                .and_then(hyper::http::uri::Authority::port_u16)
                .unwrap_or(if config.use_tls { 443 } else { 80 });
            if !self.network.names_peer(host, port) {
                return Err(ErrorCode::HttpRequestDenied.into());
            }
            Ok(default_send_request(request, config))
        }
    }

    pub(super) fn start(
        runtime: &tokio::runtime::Handle,
        pre: InstancePre<Guest>,
        recipe: Arc<Recipe>,
        port: Option<u16>,
        stop: watch::Receiver<bool>,
        exit: watch::Sender<Option<WasmExit>>,
    ) -> Result<(), WasmEngineError> {
        let port = port.ok_or(WasmEngineError::ServePortMissing)?;
        let pre = ProxyPre::new(pre).map_err(instantiate)?;
        let bound = std::net::TcpListener::bind((Ipv4Addr::UNSPECIFIED, port))
            .and_then(|l| l.set_nonblocking(true).map(|()| l))
            .map_err(|e| WasmEngineError::Io(format!("bind port {port}: {e}")))?;
        let _entered = runtime.enter();
        let listener = tokio::net::TcpListener::from_std(bound)
            .map_err(|e| WasmEngineError::Io(format!("listen on port {port}: {e}")))?;
        runtime.spawn(serve(listener, pre, recipe, stop, exit));
        Ok(())
    }

    async fn serve(
        listener: tokio::net::TcpListener,
        pre: ProxyPre<Guest>,
        recipe: Arc<Recipe>,
        stop: watch::Receiver<bool>,
        exit: watch::Sender<Option<WasmExit>>,
    ) {
        let mut connections = tokio::task::JoinSet::new();
        let outcome = loop {
            tokio::select! {
                () = stopped(stop.clone()) => break WasmExit::Stopped,
                accepted = listener.accept() => match accepted {
                    Ok((socket, _)) => {
                        let pre = pre.clone();
                        let recipe = Arc::clone(&recipe);
                        let stop = stop.clone();
                        connections.spawn(async move {
                            let service = hyper::service::service_fn(move |req| {
                                let pre = pre.clone();
                                let recipe = Arc::clone(&recipe);
                                let stop = stop.clone();
                                async move { Ok::<_, Infallible>(handle(pre, recipe, stop, req).await) }
                            });
                            let _ = hyper::server::conn::http1::Builder::new()
                                .keep_alive(true)
                                .serve_connection(TokioIo::new(socket), service)
                                .await;
                        });
                    }
                    Err(e) => break WasmExit::Trapped { message: format!("accept: {e}") },
                },
            }
        };
        connections.abort_all();
        drop(listener);
        recipe.sink.finish();
        exit.send_replace(Some(outcome));
    }

    async fn handle(
        pre: ProxyPre<Guest>,
        recipe: Arc<Recipe>,
        stop: watch::Receiver<bool>,
        req: hyper::Request<hyper::body::Incoming>,
    ) -> hyper::Response<HyperOutgoingBody> {
        match answer(pre, Arc::clone(&recipe), stop, req).await {
            Ok(response) => response,
            Err(message) => {
                recipe
                    .sink
                    .write(GuestStream::Stderr, format!("{message}\n").as_bytes());
                let mut response = hyper::Response::new(empty());
                *response.status_mut() = hyper::StatusCode::INTERNAL_SERVER_ERROR;
                response
            }
        }
    }

    async fn answer(
        pre: ProxyPre<Guest>,
        recipe: Arc<Recipe>,
        stop: watch::Receiver<bool>,
        req: hyper::Request<hyper::body::Incoming>,
    ) -> Result<hyper::Response<HyperOutgoingBody>, String> {
        let mut store = recipe.store(pre.engine()).map_err(|e| e.to_string())?;
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let incoming = store
            .data_mut()
            .new_incoming_request(Scheme::Http, req)
            .map_err(|e| format!("{e:#}"))?;
        let out = store
            .data_mut()
            .new_response_outparam(sender)
            .map_err(|e| format!("{e:#}"))?;
        let deadline = recipe.limits.deadline;
        let task = tokio::spawn(async move {
            let run = async {
                let proxy = pre.instantiate_async(&mut store).await?;
                proxy
                    .wasi_http_incoming_handler()
                    .call_handle(&mut store, incoming, out)
                    .await
                    .map(Ok)
            };
            tokio::select! {
                result = run => classify(result),
                after = elapsed(deadline) => WasmExit::Deadline { after },
                () = stopped(stop) => WasmExit::Stopped,
            }
        });
        match receiver.await {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(code)) => Err(format!("the guest answered with {code:?}")),
            Err(_) => Err(match task.await {
                Ok(WasmExit::Code { code: 0 }) => {
                    "the guest returned without setting a response".to_string()
                }
                Ok(exit) => format!("the guest ended without a response: {exit:?}"),
                Err(e) => format!("the request task failed: {e}"),
            }),
        }
    }

    fn empty() -> HyperOutgoingBody {
        http_body_util::Empty::<Bytes>::new()
            .map_err(|never: Infallible| match never {})
            .boxed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::Peer;

    fn net(bind: &[u16], connect: &[(&str, u16)]) -> NetworkGrant {
        NetworkGrant {
            bind: bind.to_vec(),
            connect: connect.iter().map(|(h, p)| Peer::new(*h, *p)).collect(),
        }
    }

    fn at(s: &str) -> SocketAddr {
        s.parse().expect("socket address")
    }

    /// RED RUN (2026-10-27): `TcpBind` arm returning `true` → `a bind on an
    /// undeclared port was admitted`.
    #[tokio::test]
    async fn bind_is_admitted_only_on_a_declared_port() {
        let n = net(&[8080], &[]);
        assert!(admits_socket(&n, at("0.0.0.0:8080"), SocketAddrUse::TcpBind).await);
        assert!(
            !admits_socket(&n, at("0.0.0.0:9090"), SocketAddrUse::TcpBind).await,
            "a bind on an undeclared port was admitted"
        );
        assert!(
            !admits_socket(&n, at("0.0.0.0:0"), SocketAddrUse::TcpBind).await,
            "an ephemeral bind was admitted with no peer declared"
        );
    }

    /// RED RUN (2026-10-27): the port filter removed from the connect arm →
    /// `a connect to an undeclared port of a declared host was admitted`.
    #[tokio::test]
    async fn connect_is_admitted_only_to_a_declared_peer() {
        let n = net(&[], &[("127.0.0.1", 5432), ("localhost", 6379)]);
        assert!(admits_socket(&n, at("127.0.0.1:5432"), SocketAddrUse::TcpConnect).await);
        assert!(
            admits_socket(&n, at("[::ffff:127.0.0.1]:5432"), SocketAddrUse::TcpConnect).await,
            "an IPv4-mapped address of a declared peer was refused"
        );
        assert!(
            !admits_socket(&n, at("127.0.0.1:22"), SocketAddrUse::TcpConnect).await,
            "a connect to an undeclared port of a declared host was admitted"
        );
        assert!(!admits_socket(&n, at("10.0.0.9:5432"), SocketAddrUse::TcpConnect).await);
        assert!(admits_socket(&n, at("127.0.0.1:6379"), SocketAddrUse::TcpConnect).await);
        assert!(admits_socket(&n, at("0.0.0.0:0"), SocketAddrUse::TcpBind).await);
    }

    #[tokio::test]
    async fn a_closed_grant_admits_nothing() {
        let n = NetworkGrant::default();
        for (addr, use_) in [
            ("0.0.0.0:80", SocketAddrUse::TcpBind),
            ("127.0.0.1:80", SocketAddrUse::TcpConnect),
            ("127.0.0.1:53", SocketAddrUse::UdpOutgoingDatagram),
        ] {
            assert!(!admits_socket(&n, at(addr), use_).await, "{addr} was admitted");
        }
    }

    /// RED RUN (2026-10-27): `memory_growing` returning `Ok(true)` before the
    /// check → `growth past the ceiling was allowed`.
    #[test]
    fn the_ceiling_counts_every_memory_in_the_store() {
        let mut c = Ceiling::new(Some(100));
        assert!(c.memory_growing(0, 60, None).expect("first memory fits"));
        let err = c
            .memory_growing(0, 60, None)
            .expect_err("growth past the ceiling was allowed");
        assert!(err.downcast_ref::<CeilingHit>().is_some());
        assert!(c.memory_growing(60, 100, None).expect("growth to the limit fits"));
    }

    #[test]
    fn an_exit_is_read_from_the_trap_that_carries_it() {
        assert_eq!(classify(Ok(Ok(()))), WasmExit::Code { code: 0 });
        assert_eq!(classify(Ok(Err(()))), WasmExit::Code { code: 1 });
        assert_eq!(
            classify(Err(anyhow::Error::new(I32Exit(7)).context("in run"))),
            WasmExit::Code { code: 7 }
        );
        assert_eq!(
            classify(Err(anyhow::Error::new(CeilingHit { limit: 64 }).context("grow"))),
            WasmExit::MemoryLimit { limit_bytes: 64 }
        );
    }

    #[test]
    fn a_component_with_no_entry_is_refused_naming_its_exports() {
        let err = World::of(["wasi:sockets/tcp@0.2.0"].into_iter()).expect_err("no entry");
        assert!(matches!(err, WasmEngineError::NoEntry { exports } if exports.len() == 1));
        assert_eq!(
            World::of(["wasi:cli/run@0.2.6"].into_iter()).expect("command"),
            World::Command
        );
    }
}
