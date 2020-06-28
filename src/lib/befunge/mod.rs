
mod process;
mod ops;
pub use self::process::*;
pub use self::ops::*;

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::VecDeque;
use std::rc::Rc;

enum MessageQueue {
    Empty,
    ReadBlocked(VecDeque<u64>),
    WriteBlocked(VecDeque<(u64, u8)>)
}

pub struct Engine {
    next_pid: u64,
    procs: BTreeMap<u64,Process>,
    buffers: BTreeMap<Rc<str>, (Rc<str>, MessageQueue)>,
    active: Vec<u64>,
    sleeping: Vec<(u64, u8)>,
    ops: OpSet
}

#[derive(Debug, PartialEq, Eq)]
pub enum EventLog {
    NewProcess(u64),
    ProcessPrintChar(u64, u8),
    ProcessPrintNum(u64, u8),
    Finished(u64),
    Crashed(u64, &'static str),
}

impl Engine {
    pub fn new() -> Engine {
        Engine { next_pid: 1,
                 procs: BTreeMap::new() ,
                 buffers: BTreeMap::new(),
                 active: Vec::new(),
                 sleeping: Vec::new(),
                 ops: OpSet::new() }
    }

    fn new_pid(&mut self) -> u64 {
        let pid = self.next_pid;
        self.next_pid += 1;
        pid
    }

    fn make_process(&mut self, input: &str, output: &str,
                    prog: Prog) -> u64 {
        let pid = self.new_pid();

        let ik = match self.buffers.get(input) {
            None => {
                let k = Rc::from(input);
                self.buffers.insert(Rc::clone(&k),
                                    (Rc::clone(&k), MessageQueue::Empty));
                k
            },
            Some((k, v)) => k.clone()
        };
        let ok = match self.buffers.get(output) {
            None => {
                let k = Rc::from(output);
                self.buffers.insert(Rc::clone(&k),
                                    (Rc::clone(&k), MessageQueue::Empty));
                k
            },
            Some((k, v)) => k.clone()
        };

        let proc = Process::new(pid, ik, ok, prog);

        self.procs.insert(pid, proc);
        self.active.push(pid);
        pid
    }

    fn step(&mut self) -> Vec<EventLog> {
        let mut log = Vec::new();
        let mut new_active = Vec::with_capacity(self.active.len());
        let mut new_sleeping = Vec::with_capacity(self.sleeping.len());

        for &(pid, c) in self.sleeping.iter() {
            if c == 0 {
                self.procs.get_mut(&pid).map(|p| p.resume(None));
                self.active.push(pid);
            } else {
                new_sleeping.push((pid, c - 1));
            }
        }
        self.sleeping = new_sleeping;

        for pid in self.active.iter() {
            let proc = match self.procs.get_mut(pid) {
                None => continue,
                Some(p) => p
            };
            self.ops.apply_to(proc);
            match proc.state() {
                ProcessState::Running(_) => new_active.push(proc.pid),
                ProcessState::Trap(Syscall::Fork) => {
                    let pid = self.next_pid;
                    self.next_pid += 1;
                    let mut p2 = proc.fork(pid);
                    proc.resume(Some(0));
                    p2.resume(Some(1));
                    new_active.push(proc.pid);
                    new_active.push(p2.pid);
                    log.push(EventLog::NewProcess(p2.pid));
                },
                ProcessState::Trap(Syscall::Sleep(c)) => {
                    if c == 0 {
                        proc.resume(None);
                        new_active.push(proc.pid);
                    } else {
                        self.sleeping.push((proc.pid, c));
                    }
                },
                ProcessState::Trap(Syscall::Send(c)) => {
                    let mut tup = self.buffers.entry(proc.output.clone())
                        .or_insert((proc.output.clone(), MessageQueue::Empty));
                    match &mut tup.1 {
                        MessageQueue::Empty => {
                            let mut q = VecDeque::new();
                            q.push_back((proc.pid, c));
                            tup.1 = MessageQueue::WriteBlocked(q);
                        },
                        MessageQueue::WriteBlocked(q) => {
                            q.push_back((proc.pid, c));
                        },
                        MessageQueue::ReadBlocked(q) => {
                            proc.resume(None);
                            new_active.push(proc.pid);
                            let blocked = q.pop_front()
                                .expect("Empty ReadBlocked Queue");
                            let blproc = self.procs.get_mut(&blocked)
                                .expect("Blocked process not found");
                            blproc.resume(Some(c));
                            new_active.push(blproc.pid);
                            if q.len() == 0 {
                                tup.1 = MessageQueue::Empty;
                            }
                        },
                        _ => panic!("foo"),
                    };
                },
                ProcessState::Trap(Syscall::Receive) => {
                    let mut tup = self.buffers.entry(proc.input.clone())
                        .or_insert((proc.input.clone(), MessageQueue::Empty));
                    match &mut tup.1 {
                        MessageQueue::Empty => {
                            let mut q = VecDeque::new();
                            q.push_back(proc.pid);
                            tup.1 = MessageQueue::ReadBlocked(q);
                        },
                        MessageQueue::ReadBlocked(q) => {
                            q.push_back(proc.pid);
                        },
                        MessageQueue::WriteBlocked(q) => {
                            let (blocked, c) = q.pop_front()
                                .expect("Empty ReadBlocked Queue");
                            proc.resume(Some(c));
                            new_active.push(proc.pid);
                            let blproc = self.procs.get_mut(&blocked)
                                .expect("Blocked process not found");
                            blproc.resume(None);
                            new_active.push(blproc.pid);
                        },
                    }
                },
                ProcessState::Trap(Syscall::PrintChar(c)) => {
                    log.push(EventLog::ProcessPrintChar(proc.pid, c));
                    proc.resume(None);
                    new_active.push(proc.pid);
                }
                ProcessState::Trap(Syscall::PrintNum(c)) => {
                    log.push(EventLog::ProcessPrintNum(proc.pid, c));
                    proc.resume(None);
                    new_active.push(proc.pid);
                }
                ProcessState::Finished => {
                    log.push(EventLog::Finished(proc.pid));
                },
                ProcessState::Crashed(msg) => {
                    log.push(EventLog::Crashed(proc.pid, msg));
                }
                s => panic!("Unhandled state: {:?}", s),
            }
        };

        self.active = new_active;
        log
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn send_receive_basic() {
        let mut eng = Engine::new();
        eng.make_process("a", "b", Prog::parse(">  5.@")
            .expect("Parse failed."));
        eng.make_process("b", "a", Prog::parse(">~ &@")
            .expect("Parse failed."));
        for i in 1..6  {
            eng.step();
        }
        assert!(eng.step() == vec![EventLog::Finished(1)]);
        assert!(eng.step() == vec![EventLog::ProcessPrintNum(2, 5)]);
        assert!(eng.step() == vec![EventLog::Finished(2)]);
    }
}
