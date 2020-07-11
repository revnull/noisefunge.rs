
mod process;
mod ops;
mod charmap;
pub use self::process::*;
pub use self::ops::*;
pub use self::charmap::*;
use crate::api::{EngineState, ProcState};

use arr_macro::arr;
use std::collections::{BTreeMap, HashSet, HashMap, VecDeque};
use std::mem;
use std::rc::Rc;
use serde::{Serialize, Deserialize};

#[derive(Debug)]
enum MessageQueue {
    Empty,
    ReadBlocked(VecDeque<u64>),
    WriteBlocked(VecDeque<(u64, u8)>)
}

pub struct Engine {
    beat: u64,
    next_pid: u64,
    progs: HashSet<Rc<Prog>>,
    procs: BTreeMap<u64,Process>,
    buffers: [MessageQueue; 256],
    active: Vec<u64>,
    sleeping: Vec<(u64, u8)>,
    ops: OpSet,
    charmap: CharMap,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventLog {
    NewProcess(u64),
    ProcessPrintChar(u64, u8),
    ProcessPrintNum(u64, u8),
    Finished(u64),
    Crashed(u64, &'static str),
}

impl Engine {
    pub fn new() -> Engine {
        Engine { beat: 0,
                 next_pid: 1,
                 progs: HashSet::new(),
                 procs: BTreeMap::new(),
                 buffers: arr![MessageQueue::Empty; 256],
                 active: Vec::new(),
                 sleeping: Vec::new(),
                 ops: OpSet::new(),
                 charmap: CharMap::default() }
    }

    fn new_pid(&mut self) -> u64 {
        let pid = self.next_pid;
        self.next_pid += 1;
        pid
    }

    pub fn make_process(&mut self, prog: Prog) -> u64 {
        let pid = self.new_pid();

        let rcprog = Rc::new(prog);
        let prog = match self.progs.get(&rcprog) {
            None => {
                self.progs.insert(Rc::clone(&rcprog));
                rcprog
            },
            Some(p) => Rc::clone(p)
        };

        let proc = Process::new(pid, prog);

        self.procs.insert(pid, proc);
        self.active.push(pid);
        pid
    }

    pub fn step(&mut self) -> Vec<EventLog> {
        let mut log = Vec::new();
        let sleeping = mem::take(&mut self.sleeping);
        let mut active = mem::take(&mut self.active);

        for pid in active.iter() {
            let proc = self.procs.get_mut(pid).expect(
                &format!("Lost pid: {}", pid));
            self.ops.apply_to(proc, None);
        }

        for &(pid, c) in sleeping.iter() {
            if c == 0 {
                self.procs.get_mut(&pid).map(|p| p.resume(None) );
                active.push(pid);
            } else {
                self.sleeping.push((pid, c - 1));
            }
        }

        let mut dead = Vec::new();
        while !active.is_empty() {
            let mut next_active = Vec::new();

            for pid in active.iter() {
                let proc = self.procs.get_mut(pid).expect(
                    &format!("Lost pid: {}", pid));
                match proc.state() {
                    ProcessState::Running(_) => {
                        proc.step();
                        self.active.push(proc.pid);
                    },
                    ProcessState::Trap(Syscall::Fork) => {
                        let pid = self.next_pid;
                        self.next_pid += 1;
                        let mut p2 = proc.fork(pid);
                        proc.resume(Some(0));
                        p2.resume(Some(1));
                        next_active.push(proc.pid);
                        next_active.push(p2.pid);
                        log.push(EventLog::NewProcess(p2.pid));
                        self.procs.insert(p2.pid, p2);
                    },
                    ProcessState::Trap(Syscall::Sleep(dur)) => {
                        let dur = *dur;
                        if dur == 0 {
                            proc.resume(None);
                            next_active.push(proc.pid);
                        } else {
                            self.sleeping.push((proc.pid, dur - 1));
                        }
                    },
                    ProcessState::Trap(Syscall::Pause) => {
                        proc.resume(None);
                        self.active.push(proc.pid);
                    },
                    ProcessState::Trap(Syscall::Send(chan, c)) => {
                        let i = *chan as usize;
                        let c = *c;
                        let buf = &mut self.buffers[i];
                        match buf {
                            MessageQueue::Empty => {
                                let mut q = VecDeque::new();
                                q.push_back((proc.pid, c));
                                *buf = MessageQueue::WriteBlocked(q);
                            },
                            MessageQueue::WriteBlocked(q) => {
                                q.push_back((proc.pid, c));
                            },
                            MessageQueue::ReadBlocked(q) => {
                                proc.resume(None);
                                next_active.push(proc.pid);
                                let blocked = q.pop_front()
                                    .expect("Empty ReadBlocked Queue");
                                let blproc = self.procs.get_mut(&blocked)
                                    .expect("Blocked process not found");
                                blproc.resume(Some(c));
                                next_active.push(blproc.pid);
                                if q.len() == 0 {
                                    *buf = MessageQueue::Empty;
                                }
                            },
                        };
                    },
                    ProcessState::Trap(Syscall::Receive(ch)) => {
                        let i = *ch as usize;
                        let buf = &mut self.buffers[i];
                        match buf {
                            MessageQueue::Empty => {
                                let mut q = VecDeque::new();
                                q.push_back(proc.pid);
                                *buf = MessageQueue::ReadBlocked(q);
                            },
                            MessageQueue::ReadBlocked(q) => {
                                q.push_back(proc.pid);
                            },
                            MessageQueue::WriteBlocked(q) => {
                                let (blocked, c) = q.pop_front()
                                    .expect("Empty ReadBlocked Queue");
                                proc.resume(Some(c));
                                next_active.push(proc.pid);
                                let blproc = self.procs.get_mut(&blocked)
                                    .expect("Blocked process not found");
                                blproc.resume(None);
                                next_active.push(blproc.pid);
                                if q.len() == 0 {
                                    *buf = MessageQueue::Empty;
                                }
                            },
                        };
                    },
                    ProcessState::Trap(Syscall::Defop(c)) => {
                        let top = proc.top().unwrap();
                        let pc = top.pc;
                        let dir = top.dir;
                        let mem = Rc::clone(&top.memory);
                        log.push(EventLog::Finished(proc.pid));
                        dead.push(proc.pid);
                        self.ops.defop(*c, Op::new(Rc::new( move |p| {
                            p.call(Rc::clone(&mem), pc, dir);
                        })));
                    },
                    ProcessState::Trap(Syscall::Call(c)) => {
                        self.ops.apply_to(proc, Some(*c));
                        proc.resume(None);
                        next_active.push(proc.pid);
                    },
                    ProcessState::Trap(Syscall::PrintChar(c)) => {
                        log.push(EventLog::ProcessPrintChar(proc.pid, *c));
                        proc.resume(None);
                        next_active.push(proc.pid);
                    },
                    ProcessState::Trap(Syscall::PrintNum(c)) => {
                        log.push(EventLog::ProcessPrintNum(proc.pid, *c));
                        proc.resume(None);
                        next_active.push(proc.pid);
                    },
                    ProcessState::Finished => {
                        log.push(EventLog::Finished(proc.pid));
                        dead.push(proc.pid);
                    },
                    ProcessState::Crashed(msg) => {
                        log.push(EventLog::Crashed(proc.pid, msg));
                        dead.push(proc.pid);
                    }
                    s => panic!("Unhandled state: {:?}", s),
                }
            }
            /*
                s => panic!("Unhandled state: {:?}", s),
                */
            active = next_active;
        }

        for pid in dead {
            self.procs.remove(&pid);
        }

        self.beat += 1;

        log
    }

    pub fn state(&self) -> EngineState {
        let mut progs = Vec::new();
        let mut prog_map : HashMap<Rc<Prog>, usize> = HashMap::new();
        let mut procs = HashMap:: new();

        for (pid, proc) in &self.procs {
            let top = match proc.top() {
                None => continue,
                Some(top) => top
            };
            let mem = Rc::clone(&top.memory);
            let prog_index = prog_map.entry(mem).or_insert_with(|| {
                progs.push(top.memory.state_tuple(&self.charmap));
                progs.len() - 1
            });

            let PC(pc) = top.pc;
            procs.insert(*pid, ProcState { prog: *prog_index,
                                           pc: pc,
                                           active: proc.is_running()});
        }

        EngineState { beat: self.beat,
                      progs: progs,
                      procs: procs
                    }
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn send_receive_basic() {
        let mut eng = Engine::new();
        eng.make_process(Prog::parse(">  50.@")
            .expect("Parse failed."));
        eng.make_process(Prog::parse(">0~ &@")
            .expect("Parse failed."));
        for i in 1..7  {
            eng.step();
        }
        assert!(eng.step() == vec![EventLog::Finished(1)]);
        assert!(eng.step() == vec![EventLog::ProcessPrintNum(2, 5)]);
        assert!(eng.step() == vec![EventLog::Finished(2)]);
    }
}
