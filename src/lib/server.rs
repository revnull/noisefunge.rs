
use simple_server::*;
use std::thread;
use std::sync::{Arc, Mutex, Condvar};
use crossbeam_channel::{bounded, Sender, Receiver};
use http::uri::Parts;
use serde_json::{from_str, to_vec};

use crate::config::{FungedConfig};
use crate::api::*;
use querystring::querify;

#[derive(Debug,Clone)]
pub struct Responder<T>(Arc<(Mutex<Option<T>>, Condvar)>);
unsafe impl<T: Send> Send for Responder<T> {}

impl<T> Responder<T> {
    fn new() -> Self {
        Responder(Arc::new((Mutex::new(None), Condvar::new())))
    }

    fn wait(&self) -> T {
        let Responder(arc) = self;
        let lock = &arc.0;
        let cond = &arc.1;

        let mut val = lock.lock().unwrap();
        while val.is_none() {
            val = cond.wait(val).unwrap();
        }

        val.take().unwrap()
    }

    pub fn respond(&self,t: T) {
        let Responder(arc) = self;
        let lock = &arc.0;
        let cond = &arc.1;
        let mut val = lock.lock().unwrap();
        *val = Some(t);
        cond.notify_one();
    }
}

#[derive(Debug)]
pub enum FungeRequest {
    StartProcess(String, Responder<Result<u64,String>>),
    GetState(Option<u64>, Responder<Arc<Vec<u8>>>),
}

unsafe impl Send for FungeRequest {}

pub struct ServerHandle {
    thread: thread::JoinHandle<()>,
    pub channel: Receiver<FungeRequest>
}

fn handle_GET(sender: Arc<Sender<FungeRequest>>,
              req: Request<Vec<u8>>, mut resp: ResponseBuilder)
              -> ResponseResult {

    let uri = req.uri();

    if uri.path() == "/" {
        return resp.status(200)
                   .body(Vec::from("Hello, world"))
                   .map_err(|e| Error::from(e))
    } else if uri.path() == "/state" {
        let prev = uri.query()
            .and_then(|qs| {
                for (k,v) in querify(qs) {
                    if k == "prev" {
                        return v.parse::<u64>().ok();
                    }
                };
                return None;
            });
        let responder = Responder::new();
        sender.send(FungeRequest::GetState(prev, responder.clone()));
        
        return resp.status(200).body((*responder.wait()).clone())
                   .map_err(|e| Error::from(e));
    }

    println!("{}", uri.path());
    println!("{:?}", uri.query());

    resp.status(404).body(Vec::new()).map_err(|e| Error::from(e))
}

fn new_process(sender: Arc<Sender<FungeRequest>>, body: Vec<u8>)
    -> Result<Vec<u8>, String> {

    let body = String::from_utf8(body).map_err(|e| format!("{}", e))?;
    let req: NewProcessReq =
        serde_json::from_str(&body).map_err(|e| format!("{}", e))?;

    let responder = Responder::new();
    let msg = FungeRequest::StartProcess(req.program, responder.clone());
    sender.send(msg);

    let resp = NewProcessResp { pid: responder.wait()? };
    serde_json::to_vec(&resp).map_err(|e| format!("{}", e))
}

fn handle_POST(sender: Arc<Sender<FungeRequest>>,
               req: Request<Vec<u8>>, mut resp: ResponseBuilder)
               -> ResponseResult {

    let uri = req.uri();

    if uri.path() == "/process" {
        match new_process(sender, req.into_body()) {
            Ok(bytes) => {
                return resp.status(200).body(bytes)
                           .map_err(|e| Error::from(e));
                },
            Err(s) => {
                return resp.status(400).body(s.into_bytes())
                           .map_err(|e| Error::from(e));
            }
        }
    }

    resp.status(404).body(Vec::new()).map_err(|e| Error::from(e))
}

fn handle_request(sender: Arc<Sender<FungeRequest>>, req: Request<Vec<u8>>,
                  mut resp: ResponseBuilder) -> ResponseResult {

    match *req.method() {
        Method::GET => handle_GET(sender, req, resp),
        Method::POST => handle_POST(sender, req, resp),
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

