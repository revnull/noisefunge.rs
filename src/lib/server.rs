extern crate simple_server;

use simple_server::*;
use std::thread;
use std::sync::{Arc, Mutex, Condvar};
use crossbeam_channel::{bounded, Sender, Receiver};

use crate::config::{FungedConfig};

#[derive(Debug)]
pub struct Responder<T>(Mutex<Option<T>>, Condvar);
unsafe impl<T: Send> Send for Responder<T> {}

#[derive(Debug)]
pub enum FungeRequest {
    StartProcess(String, String, String, Responder<Result<u64,String>>),
}

unsafe impl Send for FungeRequest {}

pub struct ServerHandle {
    thread: thread::JoinHandle<()>,
    pub channel: Receiver<FungeRequest>
}

fn handle_GET(sender: Arc<Sender<FungeRequest>>,
              req: Request<Vec<u8>>, mut resp: ResponseBuilder)
              -> ResponseResult {
    if req.uri() == "/" {
        return resp.status(200)
                   .body(Vec::from("Hello, world"))
                   .map_err(|e| Error::from(e))
    }

    resp.status(404).body(Vec::new()).map_err(|e| Error::from(e))
}

fn handle_request(sender: Arc<Sender<FungeRequest>>, req: Request<Vec<u8>>,
                  mut resp: ResponseBuilder) -> ResponseResult {

    match *req.method() {
        Method::GET => handle_GET(sender, req, resp),
        _ => resp.status(501).body(Vec::new()).map_err(|e| Error::from(e))
    }
}

impl ServerHandle {

    pub fn new(conf: &FungedConfig) -> ServerHandle {
        let (snd, rcv) = bounded(4);
        let snd = Arc::new(snd);
        let mut server = Server::new(move |request, mut response| {
            handle_request(Arc::clone(&snd), request, response)
        });
        server.dont_serve_static_files();

        let host = format!("{}", conf.host);
        let port = format!("{}", conf.port);
        let handle = thread::spawn(move || {
            server.listen(&host, &port);
        });

        ServerHandle { thread: handle,
                       channel: rcv }
    }
}

