//! The greeter component: a WASI preview 2 `wasi:http/proxy`. `GET /fetch/
//! <authority>` makes an outgoing request to that authority and answers with
//! its status, or with 502 and the refusal; any other request is answered
//! with 200 and `hello from <path>`.

use wasi::exports::http::incoming_handler::Guest;
use wasi::http::outgoing_handler;
use wasi::http::types::{
    Fields, IncomingRequest, OutgoingBody, OutgoingRequest, OutgoingResponse, ResponseOutparam,
    Scheme,
};

struct Greeter;

wasi::http::proxy::export!(Greeter);

impl Guest for Greeter {
    fn handle(request: IncomingRequest, outparam: ResponseOutparam) {
        let path = request.path_with_query().unwrap_or_default();
        println!("greeter {path}");
        let (status, body) = match path.strip_prefix("/fetch/") {
            Some(authority) => fetch(authority),
            None => (200, format!("hello from {path}\n")),
        };
        let response = OutgoingResponse::new(Fields::new());
        let _ = response.set_status_code(status);
        let Ok(out) = response.body() else { return };
        ResponseOutparam::set(outparam, Ok(response));
        if let Ok(stream) = out.write() {
            let _ = stream.blocking_write_and_flush(body.as_bytes());
        }
        let _ = OutgoingBody::finish(out, None);
    }
}

fn fetch(authority: &str) -> (u16, String) {
    let request = OutgoingRequest::new(Fields::new());
    let _ = request.set_scheme(Some(&Scheme::Http));
    let _ = request.set_authority(Some(authority));
    let _ = request.set_path_with_query(Some("/"));
    match outgoing_handler::handle(request, None) {
        Err(e) => (502, format!("refused: {e:?}\n")),
        Ok(pending) => {
            pending.subscribe().block();
            match pending.get() {
                Some(Ok(Ok(response))) => (200, format!("fetched {}\n", response.status())),
                Some(Ok(Err(e))) => (502, format!("refused: {e:?}\n")),
                _ => (502, "refused: no response\n".to_string()),
            }
        }
    }
}
