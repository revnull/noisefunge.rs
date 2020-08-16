
use log::*;
use std::fs::{File, OpenOptions};
use std::mem;
use std::process::{Child, Command, Stdio};
use crate::config::SubprocessCommand;

pub struct SubprocessHandle(Vec<(String, Child)>);

impl Drop for SubprocessHandle {
    fn drop(&mut self) {
        let mut remaining = Vec::new();

        for (name, mut sub) in self.0.drain(..) {
            info!("Killing process: {}", name);
            match sub.kill() {
                Ok(_) => {},
                Err(_) => error!("Failed to kill {}", name),
            }
            remaining.push((name, sub));
        }
        
        let mut attempts = 10;
        while attempts > 0 && !remaining.is_empty() {
            let mut temp = mem::take(&mut remaining);
            for (name, mut sub) in temp.drain(..) {
                match sub.try_wait() {
                    Ok(Some(st)) => {
                        info!("{} exited with status: {}", name, st)
                    },
                    _ => {
                        debug!("Still waiting on {}", name);
                        remaining.push((name, sub));
                    }
                }
            }
            attempts -= 1;
        }

        if !remaining.is_empty() {
            for (name, _) in &remaining {
                error!("Failed to kill {}", name);
            }
        }
    }
}

impl SubprocessHandle {
    pub fn new(procs: &Vec<SubprocessCommand>) -> Self {
        let mut res = Vec::new();

        for sub in procs {
            let stdin = match sub.stdin.as_ref() {
                None => { Stdio::null() },
                Some(f) => { File::open(f)
                                  .expect(&format!("Failed to open {}", f))
                                  .into() }
            };

            let stdout = match sub.stdout.as_ref() {
                None => { Stdio::inherit() },
                Some(f) =>
                    OpenOptions::new()
                                .create(true)
                                .append(true)
                                .open(f)
                                .expect(&format!("Failed to open {}", f))
                                .into(),
            };

            let stderr = match sub.stderr.as_ref() {
                None => { Stdio::inherit() },
                Some(f) =>
                    OpenOptions::new()
                                .create(true)
                                .append(true)
                                .open(f)
                                .expect(&format!("Failed to open {}", f))
                                .into(),
            };

            let child = Command::new(&sub.command[0])
                                .args(&sub.command[1..])
                                .stdin(stdin)
                                .stdout(stdout)
                                .stderr(stderr)
                                .spawn()
                                .expect(&format!("Failed to start {}", sub.name));

            res.push((sub.name.clone(), child));
        }

        SubprocessHandle(res)
    }

    pub fn check_children(&mut self) {
        let mut children = mem::take(&mut self.0);

        for (name, mut c) in children.drain(..) {
            match c.try_wait() {
                Ok(Some(status)) => {
                    warn!("Child {} exited with status {}", name, status);
                },
                Ok(None) => {
                    self.0.push((name, c));
                },
                Err(e) => {
                    warn!("Could not wait on child {}: {}", name, e);
                    self.0.push((name, c));
                }
            }
        }
    }
}
