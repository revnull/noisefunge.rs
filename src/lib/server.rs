extern crate simple_server;

use simple_server::Server;
use std::thread;
use std::sync::{Arc, Mutex};

use crate::config::{FungedConfig};
use crate::befunge::{Engine};

pub struct ServerHandle {
    thread: thread::JoinHandle<()>
}

impl ServerHandle {

    pub fn new(conf: &FungedConfig, eng: Arc<Mutex<Engine>>) -> ServerHandle {
        let mut server = Server::new(move |request, mut response| {
            panic!("panic!")
        });
        server.dont_serve_static_files();

        let host = format!("{}", conf.host);
        let port = format!("{}", conf.port);
        let handle = thread::spawn(move || {
            server.listen(&host, &port);
        });

        ServerHandle { thread : handle }
    }
}

