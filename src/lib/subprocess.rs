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

use log::*;
use std::fs::{File, OpenOptions};
use std::mem;
use std::process::{Child, Command, Stdio};
use crate::config::SubprocessCommand;

pub struct SubprocessHandle(Vec<(SubprocessCommand, Option<Child>)>);

impl Drop for SubprocessHandle {
    fn drop(&mut self) {
        let mut remaining = Vec::new();

        for (sc, subopt) in self.0.drain(..) {
            let mut sub = match subopt {
                None => continue,
                Some(s) => s
            };
            info!("Killing process: {}", sc.name);
            match sub.kill() {
                Ok(_) => {},
                Err(_) => error!("Failed to kill {}", sc.name),
            }
            remaining.push((sc.name, sub));
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

fn launch_child(sub: &SubprocessCommand) -> Child {
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

    Command::new(&sub.command[0])
            .args(&sub.command[1..])
            .stdin(stdin)
            .stdout(stdout)
            .stderr(stderr)
            .spawn()
            .expect(&format!("Failed to start {}", sub.name))

}

impl SubprocessHandle {
    pub fn new(procs: Vec<SubprocessCommand>) -> Self {
        let mut res = Vec::new();

        for sub in procs {
            let ch = launch_child(&sub);
            res.push((sub, Some(ch)))
        }

        SubprocessHandle(res)
    }

    pub fn check_children(&mut self) {
        let mut children = mem::take(&mut self.0);

        for (sc, child) in children.drain(..) {
            match child {
                None => self.0.push((sc, None)),
                Some(mut c) => {
                    match c.try_wait() {
                        Ok(Some(status)) => {
                            warn!("Child {} exited with status {}",
                                  sc.name, status);
                            if sc.reload {
                                warn!("Restarting {}", sc.name);
                                let ch = launch_child(&sc);
                                self.0.push((sc, Some(ch)))
                            } else {
                                self.0.push((sc, None))
                            }
                        },
                        Ok(None) => {
                            self.0.push((sc, Some(c)));
                        },
                        Err(e) => {
                            warn!("Could not wait on child {}: {}",
                                  sc.name, e);
                            self.0.push((sc, Some(c)));
                        }
                    }
                }
            }
        }
    }
}
