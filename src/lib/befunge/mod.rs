
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

impl MessageQueue {
    pub fn reset(&mut self) {
        *self = MessageQueue::Empty;
    }

    pub fn retain<F>(&mut self, mut f: F)
        where F: FnMut(&u64) -> bool
    {
        let empty = match self {
            MessageQueue::ReadBlocked(ref mut q) => {
                q.retain(|p| f(p));
                q.is_empty()
            },
            MessageQueue::WriteBlocked(ref mut q) => {
                q.retain(|(p, _)| f(p));
                q.is_empty()
            },
            _ => { true }
        };
        if empty { *self = MessageQueue::Empty };
    }

    pub fn read(&mut self, pid: u64) -> Option<(u64, u8)> {
        match self {
            MessageQueue::ReadBlocked(q) => {
                q.push_back(pid);
                None
            },
            MessageQueue::WriteBlocked(q) => {
                let res = q.pop_front().expect("Invalid WriteBlocked queue");
                if q.is_empty() {
                    *self = MessageQueue::Empty;
                }
                Some(res)
            },
            _ => { 
                let mut q = VecDeque::new();
                q.push_back(pid);
                *self = MessageQueue::ReadBlocked(q);
                None
            }
        }
    }

    pub fn write(&mut self, pid: u64, c: u8) -> Option<u64> {
        match self {
            MessageQueue::ReadBlocked(q) => {
                let res = q.pop_front().expect("Invalid ReadBlocked queue");
                if q.is_empty() {
                    *self = MessageQueue::Empty;
                }
                Some(res)
            },
            MessageQueue::WriteBlocked(q) => {
                q.push_back((pid, c));
                None
            },
            _ => { 
                let mut q = VecDeque::new();
                q.push_back((pid, c));
                *self = MessageQueue::WriteBlocked(q);
                None
            }
        }
    }
}

pub struct Engine {
    beat: u64,
    freq: u64,
    next_pid: u64,
    progs: HashSet<Rc<Prog>>,
    procs: BTreeMap<u64,Process>,
    process_names: HashSet<Rc<str>>,
    buffers: [MessageQueue; 256],
    active: Vec<u64>,
    sleeping: Vec<(u64, u32)>,
    all_killed: bool,
    killed: HashSet<u64>,
    ops: OpSet,
    charmap: CharMap,
    crash_log: Vec<(u64, CrashReason)>
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub enum EventLog {
    NewProcess(u64),
    PrintChar(u64, u8),
    PrintNum(u64, u8),
    Play(Note),
    Finished(u64),
    Crashed(u64, CrashReason),
    Killed(u64)
}

impl Engine {
    pub fn new(period: u64) -> Engine {
        Engine { beat: 0,
                 freq: 24 / period,
                 next_pid: 1,
                 progs: HashSet::new(),
                 procs: BTreeMap::new(),
                 process_names: HashSet::new(),
                 buffers: arr![MessageQueue::Empty; 256],
                 active: Vec::new(),
                 sleeping: Vec::new(),
                 all_killed: false,
                 killed: HashSet::new(),
                 ops: OpSet::default(),
                 charmap: CharMap::default(),
                 crash_log: Vec::new() }
    }

    fn new_pid(&mut self) -> u64 {
        let pid = self.next_pid;
        self.next_pid += 1;
        pid
    }

    pub fn kill(&mut self, pid: u64) {
        self.killed.insert(pid);
    }

    pub fn kill_all(&mut self) {
        self.all_killed = true
    }

    pub fn make_process(&mut self, name: Option<String>, prog: Prog) -> u64 {
        let pid = self.new_pid();

        let rcprog = Rc::new(prog);
        let prog = match self.progs.get(&rcprog) {
            None => {
                self.progs.insert(Rc::clone(&rcprog));
                rcprog
            },
            Some(p) => Rc::clone(p)
        };

        let name = name.map(|n| {
            let rcname = Rc::from(n);
            match self.process_names.get(&rcname) {
                Some(n) => Rc::clone(n),
                None => {
                    self.process_names.insert(Rc::clone(&rcname));
                    rcname
                }
            }
        });

        let proc = Process::new(pid, name, prog);

        self.procs.insert(pid, proc);
        self.active.push(pid);
        pid
    }

    pub fn step(&mut self) -> (u64, Vec<EventLog>) {
        let mut log = Vec::new();
        let sleeping = mem::take(&mut self.sleeping);
        let mut active = mem::take(&mut self.active);
        let all_killed = self.all_killed;
        self.all_killed = false;
        let mut dead = Vec::new();
        let killed = mem::take(&mut self.killed);
        let oldbeat = self.beat;
        self.beat += 1;
        self.crash_log = Vec::new();

        if all_killed {
            for pid in self.procs.keys() {
                log.push(EventLog::Killed(*pid));
            }
            self.procs = BTreeMap::new();
            for i in 0..255 {
                self.buffers[i].reset();
            }
            return (oldbeat, log)
        }

        let mut needs_filter = HashSet::new();
        for pid in &killed {
            let proc = match self.procs.get_mut(&pid) {
                Some(p) => p,
                _ => continue,
            };
            match proc.kill() {
                ProcessState::Trap(Syscall::Send(ch, _)) => {
                    needs_filter.insert(ch);
                    active.push(*pid);
                },
                ProcessState::Trap(Syscall::Receive(ch)) => {
                    needs_filter.insert(ch);
                    active.push(*pid);
                },
                _ => ()
            }
        }

        for ch in needs_filter {
            self.buffers[ch as usize].retain(|ref p| !killed.contains(p));
        }

        for pid in active.iter() {
            let proc = self.procs.get_mut(pid).expect(
                &format!("Lost pid: {}", pid));
            proc.clear_output();
            match proc.state() {
                ProcessState::Running(true) => {
                    match proc.peek() {
                        None => proc.die(CrashReason::OutOfBounds(None)),
                        Some(34) => proc.set_state(
                            ProcessState::Running(false)),
                        Some(c) => proc.push(c)
                    }
                },
                ProcessState::Running(false) => {
                    self.ops.apply_to(proc, None);
                },
                ProcessState::Killed => { },
                _ => panic!("Process in active list is not running")
            }
        }

        for &(pid, c) in sleeping.iter() {
            let proc = match self.procs.get_mut(&pid) {
                Some(p) => p,
                _ => continue
            };
            if *proc.state() == ProcessState::Killed {
                active.push(pid);
            } else if c == 0 {
                proc.resume(None);
                active.push(pid);
            } else {
                self.sleeping.push((pid, c - 1));
            }
        }

        while !active.is_empty() {
            let mut next_active = Vec::new();

            for pid in active.iter() {
                let proc = self.procs.get_mut(pid).expect(
                    &format!("Lost pid: {}", pid));
                match proc.state() {
                    ProcessState::Running(_) => {
                        proc.step();
                        match &proc.state() {
                            ProcessState::Running(_) =>
                                self.active.push(proc.pid),
                            ProcessState::Crashed(msg) => {
                                log.push(EventLog::Crashed(proc.pid, *msg));
                                self.crash_log.push((proc.pid, *msg));
                                dead.push(proc.pid);
                            },
                            _ => dead.push(proc.pid),
                        }
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
                    ProcessState::Trap(Syscall::Quantize(q)) => {
                        let q = *q as u64;
                        let quarter = oldbeat / self.freq;
                        let needed = match quarter % q {
                            0 => 0,
                            n => q - n
                        };
                        let sub = match oldbeat % self.freq {
                            0 => 0,
                            n => self.freq - n
                        };
                        let n = ((needed * self.freq) + sub) as u32;
                        proc.set_state(ProcessState::Trap(Syscall::Sleep(n)));
                        next_active.push(proc.pid);
                    },
                    ProcessState::Trap(Syscall::Pause) => {
                        proc.resume(None);
                        self.active.push(proc.pid);
                    },
                    ProcessState::Trap(Syscall::Send(chan, c)) => {
                        let i = *chan as usize;
                        let c = *c;
                        let buf = &mut self.buffers[i];
                        match buf.write(proc.pid, c) {
                            Some(blpid) => {
                                proc.resume(None);
                                next_active.push(proc.pid);
                                let blproc = self.procs.get_mut(&blpid)
                                    .expect("Blocked process not found");
                                blproc.resume(Some(c));
                                next_active.push(blproc.pid);
                            },
                            None => { }
                        }
                    },
                    ProcessState::Trap(Syscall::Receive(ch)) => {
                        let i = *ch as usize;
                        let buf = &mut self.buffers[i];
                        match buf.read(proc.pid) {
                            Some((blpid, c)) => {
                                proc.resume(Some(c));
                                next_active.push(proc.pid);
                                let blproc = self.procs.get_mut(&blpid)
                                    .expect("Blocked process not found");
                                blproc.resume(None);
                                next_active.push(blproc.pid);
                            },
                            None => {},
                        }
                    },
                    ProcessState::Trap(Syscall::Defop(c)) => {
                        let top = proc.top().unwrap();
                        let pc = top.pc;
                        let dir = top.dir;
                        let mem = Rc::clone(&top.memory);
                        log.push(EventLog::Finished(proc.pid));
                        dead.push(proc.pid);
                        self.ops.defop(*c,
                            Op::new(Rc::new( move |p| {
                                             p.call(Rc::clone(&mem), pc, dir);
                                             }),
                                    *c, format!("User opcode {:X}", *c),
                                    "User defined opcode"));
                    },
                    ProcessState::Trap(Syscall::Call(c)) => {
                        let c = *c;
                        self.ops.apply_to(proc, Some(c));
                        proc.resume(None);
                        next_active.push(proc.pid);
                    },
                    ProcessState::Trap(Syscall::PrintChar(c)) => {
                        let c = *c;
                        log.push(EventLog::PrintChar(proc.pid, c));
                        proc.set_output(format!("{}", self.charmap[c]));
                        proc.resume(None);
                        next_active.push(proc.pid);
                    },
                    ProcessState::Trap(Syscall::PrintNum(c)) => {
                        let c = *c;
                        log.push(EventLog::PrintNum(proc.pid, c));
                        proc.set_output(format!("{:X}", c));
                        proc.resume(None);
                        next_active.push(proc.pid);
                    },
                    ProcessState::Trap(Syscall::Play(note)) => {
                        log.push(EventLog::Play(*note));
                        proc.resume(None);
                        next_active.push(proc.pid);
                    },
                    ProcessState::Finished => {
                        log.push(EventLog::Finished(proc.pid));
                        dead.push(proc.pid);
                    },
                    ProcessState::Crashed(msg) => {
                        log.push(EventLog::Crashed(proc.pid, *msg));
                        dead.push(proc.pid);
                        self.crash_log.push((proc.pid, *msg));
                    },
                    ProcessState::Killed => {
                        dead.push(proc.pid);
                    },
                }
            }

            active = next_active;
        }

        for pid in dead {
            self.procs.remove(&pid);
        }

        (oldbeat, log)
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
                                           active: proc.is_running(),
                                           output: proc.get_output() });
        }

        let mut buffers = BTreeMap::new();
        for i in 0..=255 {
            let buf = &self.buffers[i];
            let len = match buf {
                MessageQueue::Empty => { continue }
                MessageQueue::ReadBlocked(q) => { -(q.len() as i64) },
                MessageQueue::WriteBlocked(q) => { q.len() as i64 },
            };
            buffers.insert(i as u8, len);
        }

        EngineState { beat: self.beat,
                      progs: progs,
                      procs: procs,
                      sleeping: self.sleeping.len(),
                      buffers: buffers,
                      crashed: self.crash_log.clone(),
                    }
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use std::iter::FromIterator;

    fn expect_ordered(eng: &mut Engine, expect: Vec<EventLog>,
                      max_steps: u16) -> u64 {
        let mut expect = VecDeque::from(expect);
        for _i in 0..max_steps {
            let (beat, log) = eng.step();
            for l in log {
                match expect.front() {
                    None => break,
                    Some(ex) if *ex == l => { expect.pop_front(); },
                    Some(_) => { println!("Unmatched event: {:?}", l); }
                }
            }
            if expect.is_empty() { return beat }
        }

        panic!("Remaining events: {:?}", expect);
    }

    fn expect_unordered(eng: &mut Engine, expect: Vec<EventLog>,
                        max_steps: u16) -> u64 {
        let mut expect: HashSet<EventLog> =
            HashSet::from_iter(expect.iter().cloned());

        for _i in 0..max_steps {
            let (beat, log) = eng.step();
            for l in log {
                if !expect.remove(&l) {
                    println!("Unexpected event: {:?}", l);
                }
            }
            if expect.is_empty() { return beat }
        }
        panic!("Remaining events: {:?}", expect);
    }

    #[test]
    fn send_receive_basic() {
        let mut eng = Engine::new(24);
        eng.make_process(None, Prog::parse(">  50.@")
            .expect("Parse failed."));
        eng.make_process(None, Prog::parse(">0~ &@")
            .expect("Parse failed."));
        assert_eq!(8, expect_ordered(&mut eng, vec![
            EventLog::Finished(1),
            EventLog::PrintNum(2, 5),
            EventLog::Finished(2)], 20));
    }

    #[test]
    fn test_note_buf() {
        let mut eng = Engine::new(24);
        eng.make_process(None, Prog::parse(">10h05n4A*9zZU1+uZW1+wZX1+xZY1+yZ@")
            .expect("Parse failed."));

        expect_ordered(&mut eng, vec![
            EventLog::Play(Note::new(1,60,40,9)),
            EventLog::Play(Note::new(1,60,40,10)),
            EventLog::Play(Note::new(1,60,41,10)),
            EventLog::Play(Note::new(1,61,41,10)),
            EventLog::Play(Note::new(2,61,41,10))], 50);
    }

    #[test]
    fn test_math_ops() {
        let mut eng = Engine::new(24);
        eng.make_process(None, Prog::parse(">45+&@").unwrap());
        eng.make_process(None, Prog::parse(">A4-&@").unwrap());
        eng.make_process(None, Prog::parse(">45*&@").unwrap());
        eng.make_process(None, Prog::parse(">52/&@").unwrap());
        eng.make_process(None, Prog::parse(">B3%&@").unwrap());
        expect_unordered(&mut eng, vec![
            EventLog::PrintNum(1, 9),
            EventLog::PrintNum(2, 6),
            EventLog::PrintNum(3, 20),
            EventLog::PrintNum(4, 2),
            EventLog::PrintNum(5, 2),
            ], 10);

        // Test wrapping math.
        eng.make_process(None, Prog::parse(">1Fh2Fh+&@").unwrap());
        eng.make_process(None, Prog::parse(">4A-&@").unwrap());
        eng.make_process(None, Prog::parse(">09h3*&@").unwrap());

        expect_unordered(&mut eng, vec![
            EventLog::PrintNum(6, 227),
            EventLog::PrintNum(7, 250),
            EventLog::PrintNum(8, 176),
            ], 20);
    }

    #[test]
    fn test_fork() {
        let mut eng = Engine::new(24);
        eng.make_process(None, Prog::parse(">ff2*+&@").unwrap());

        expect_unordered(&mut eng, vec![
            EventLog::NewProcess(2),
            EventLog::NewProcess(3),
            EventLog::NewProcess(4),
            EventLog::PrintNum(1,0),
            EventLog::PrintNum(2,1),
            EventLog::PrintNum(3,2),
            EventLog::PrintNum(4,3),
            EventLog::Finished(1),
            EventLog::Finished(2),
            EventLog::Finished(3),
            EventLog::Finished(4),
            ], 10);
    }

    #[test]
    fn test_goto() {
        let mut eng = Engine::new(24);
        eng.make_process(None, Prog::parse("> 11G 2&@\n\
                                            @>3&@").unwrap());

        expect_ordered(&mut eng, vec![
            EventLog::PrintNum(1, 3),
            ], 10);
        
        eng.make_process(None, Prog::parse(">11G").unwrap());
        expect_ordered(&mut eng, vec![
            EventLog::Crashed(2, CrashReason::OutOfBounds(Some(71))),
            ], 10);

    }

    #[test]
    fn test_put_get_call() {
        let mut eng = Engine::new(24);
        eng.make_process(None, Prog::parse(">63h70p &@").unwrap());
        expect_ordered(&mut eng, vec![
            EventLog::PrintNum(1, 6),
            ], 10);
        eng.make_process(None, Prog::parse(">#820g&@").unwrap());
        expect_ordered(&mut eng, vec![
            EventLog::PrintNum(2, 56),
            ], 10);
        eng.make_process(None, Prog::parse(">#820c&@").unwrap());
        expect_ordered(&mut eng, vec![
            EventLog::PrintNum(3, 8),
            ], 10);
        eng.make_process(None, Prog::parse(">511p@").unwrap());
        eng.make_process(None, Prog::parse(">11g@").unwrap());
        eng.make_process(None, Prog::parse(">11c@").unwrap());
        expect_unordered(&mut eng, vec![
            EventLog::Crashed(4, CrashReason::OutOfBounds(Some(112))),
            EventLog::Crashed(5, CrashReason::OutOfBounds(Some(103))),
            EventLog::Crashed(6, CrashReason::OutOfBounds(Some(99))),
            ], 10);
    }

}
