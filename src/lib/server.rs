/*
    Noisefunge Copyright (C) 2021 Rev. Johnny Healey <rev.null@gmail.com>

    This program is free software: you can redistribute it and/or modify
    it under the terms of the GNU General Public License as published by
    the Free Software Foundation, either version 3 of the License, or
    (at your option) any later version.

    This program is distributed in the hope that it will be useful,
    but WITHOUT ANY WARRANTY; without even the implied warranty of
    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
    GNU General Public License for more details.

    You should have received a copy of the GNU General Public License
    along with this program.  If not, see <https://www.gnu.org/licenses/>.
*/

use rouille::{Request, Response, router, try_or_400};
use log::*;
use std::thread;
use std::sync::{Arc, Mutex, Condvar};
use std::time::Duration;
use crossbeam_channel::{bounded, Sender, Receiver};

use crate::config::{FungedConfig};
use crate::api::*;

#[derive(Debug,Clone)]
pub struct Responder<T>(Arc<(Mutex<Option<T>>, Condvar)>);
unsafe impl<T: Send> Send for Responder<T> {}

impl<T> Responder<T> {
    fn new() -> Self {
        Responder(Arc::new((Mutex::new(None), Condvar::new())))
    }

    fn wait(&self) -> Option<T> {
        let Responder(arc) = self;
        let lock = &arc.0;
        let cond = &arc.1;

        let (mut val, timeout) = cond.wait_timeout_while(
            lock.lock().unwrap(),
            Duration::from_secs(30),
            |value| value.is_none()).unwrap();

        if timeout.timed_out() {
            debug!("Timed out waiting for responder.");
            return None
        }
        val.take()
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
    StartProcess(Option<String>, String, Responder<Result<u64,String>>),
    GetState(Option<u64>, Responder<Option<Arc<Vec<u8>>>>),
    Kill(KillReq)
}

unsafe impl Send for FungeRequest {}

pub struct ServerHandle {
    thread: thread::JoinHandle<()>,
    pub channel: Receiver<FungeRequest>
}

fn kill(sender: &Sender<FungeRequest>, request: &Request) -> Response {
    let killreq : KillReq = try_or_400!(rouille::input::json_input(&request));
    
    sender.send(FungeRequest::Kill(killreq))
          .expect("Sender::send failed");

    Response::json(&KillResp { })
}

fn new_process(sender: &Sender<FungeRequest>, request: &Request) -> Response {
    let data: NewProcessReq = try_or_400!(rouille::input::json_input(&request));

    let responder = Responder::new();
    sender.send(FungeRequest::StartProcess(data.name, data.program,
                                           responder.clone()))
          .expect("Sender::send failed");

    match responder.wait() {
        None => Response::text("Server timed out.").with_status_code(503),
        Some(Ok(resp)) => Response::json(&NewProcessResp { pid: resp }),
        Some(Err(e)) => Response::text(format!("Bad Program: {}", e))
            .with_status_code(400),
    }
}

fn get_state(sender: &Sender<FungeRequest>, request: &Request) -> Response {
    let prev = request.get_param("prev")
                      .and_then(|p| p.parse::<u64>().ok());

    let responder = Responder::new();
    sender.send(FungeRequest::GetState(prev, responder.clone()))
          .expect("Sender::send failed");

    match responder.wait() {
        Some(Some(bytes)) =>
            Response::from_data("application/json; charset=utf-8",
                               (*bytes).clone()),
        Some(None) => Response::empty_400(),
        None => Response::text("Server timed out.").with_status_code(503),
    }
}

fn handle_request(sender: &Sender<FungeRequest>, request: &Request)
    -> Response {
    router!(request,
        (GET) (/state) => { get_state(sender, request) },
        (POST) (/process) => { new_process(sender, request) },
        (POST) (/kill) => { kill(sender, request) },

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

    pub fn shutdown(self) {
        self.thread.join().expect("Failed to join server thread");
    }
}

