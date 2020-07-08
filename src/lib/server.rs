
use rouille::{Request, Response, router, try_or_400};
use rouille::input;
use std::thread;
use std::error;
use std::fmt;
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

fn new_process(sender: &Sender<FungeRequest>, request: &Request) -> Response {
    let data = try_or_400!(rouille::input::plain_text_body(&request));

    let responder = Responder::new();
    sender.send(FungeRequest::StartProcess(data, responder.clone()));

    Response::json(&NewProcessResp { pid: responder.wait().unwrap() })
}

fn get_state(sender: &Sender<FungeRequest>, request: &Request) -> Response {
    let prev = request.get_param("prev")
                      .and_then(|p| p.parse::<u64>().ok());

    let responder = Responder::new();
    sender.send(FungeRequest::GetState(prev, responder.clone()));

    Response::from_data("application/json; charset=utf-8", 
                        (*responder.wait()).clone())
}

fn handle_request(sender: &Sender<FungeRequest>, request: &Request)
    -> Response {
    router!(request,
        (GET) (/state) => { get_state(sender, request) },
        (POST) (/process) => { new_process(sender, request) },

        _ => Response::empty_404()
    )
}

impl ServerHandle {

    pub fn new(conf: &FungedConfig) -> ServerHandle {
        let (snd, rcv) = bounded(4);

        let host = format!("{}:{}", conf.host, conf.port);
        let handle = thread::spawn(move || {
            rouille::start_server(host, move |request|
                handle_request(&snd.clone(), request));
        });

        ServerHandle { thread: handle,
                       channel: rcv }
    }
}

