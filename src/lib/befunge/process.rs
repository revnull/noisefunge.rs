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

use std::cmp::max;
use std::fmt;
use std::rc::Rc;
use std::mem;
use serde::{Serialize, Deserialize};
use super::charmap::CharMap;

#[derive(Copy, Clone, Serialize, Deserialize)]
pub enum Dir { U, D, L, R }

#[derive(Copy, Clone, Serialize, Deserialize)]
pub struct PC(pub usize);

#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Debug)]
pub struct Prog { width : usize, data : Vec<u8> }

#[derive(Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Debug)]
pub enum ParseError {
    Empty,
    TooManyLines,
    LineTooLong
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use ParseError::*;
        match self {
            Empty => write!(f, "Empty program"),
            TooManyLines => write!(f, "too many lines"),
            LineTooLong => write!(f, "too long lines"),
        }
    }
}

impl Prog {

    pub fn parse(prog: &str) -> Result<Prog, ParseError> {
        if prog.len() == 0 {
            return Err(ParseError::Empty);
        }
        let mut lines = Vec::new();
        let mut longest = 0;
        for line in prog.split("\n") {
            lines.push(line);
            longest = max(longest, line.bytes().count());
        }
        if longest == 0 {
            return Err(ParseError::Empty);
        }
        if lines.len() > 255 {
            return Err(ParseError::TooManyLines);
        }
        if longest > 255 {
            return Err(ParseError::LineTooLong);
        }
        let mut mem = Vec::new();
        for line in lines {
            let mut count = 0;
            for ch in line.bytes() {
                mem.push(ch);
                count += 1;
            }
            while count < longest {
                mem.push(32); // Pad with space
                count += 1
            }
        }
        Ok(Prog { width : longest, data : mem })
    }

    pub fn rows(&self) -> usize {
        self.data.len() / self.width
    }

    pub fn cols(&self) -> usize {
        self.width
    }

    pub fn lookup(&self, pc : PC) -> u8 {
        let PC(i) = pc;
        self.data[i]
    }

    pub fn update(&mut self, pc: PC, c: u8) {
        let PC(i) = pc;
        self.data[i] = c;
    }

    pub fn xy_to_pc(&self, x: usize, y: usize) -> Option<PC> {
        let i = (self.width * y) + x;
        if i < self.data.len() {
            Some(PC(i))
        } else {
            None
        }
    }

    pub fn state_tuple(&self, cm: &CharMap) -> (usize, String) {
        let mut res = String::new();
        for c in &self.data {
            res.push(cm[*c]);
        }
        (self.width, res)
    }
}

#[derive(Clone)]
pub struct ProcessStack {
    pub memory: Rc<Prog>,
    pub pc: PC,
    pub dir: Dir
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Syscall {
    Fork,
    Sleep(u32),
    Pause,
    PrintChar(u8),
    PrintNum(u8),
    Send(u8,u8),
    Receive(u8),
    Defop(u8),
    Call(u8),
    Play(Note),
    Quantize(u8),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
pub enum CrashReason {
    OutOfBounds(Option<u8>), // optional opcode
    InvalidOpcode(u8),
    PopFromEmptyStack,
    InvalidQuantize,
    DivideByZero
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ProcessState {
    Running(bool),
    Trap(Syscall),
    Finished,
    Crashed(CrashReason),
    Killed
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, Hash, Serialize,
         Deserialize)]
pub struct Note {
    pub cha: u8,
    pub pch: u8,
    pub vel: u8,
    pub dur: u8
}

impl Note {
    pub fn new(cha: u8, pch: u8, vel: u8, dur: u8) -> Self {
        Note { cha: cha,
               pch: pch,
               vel: vel,
               dur: dur }
    }
}

#[derive(Clone)]
pub struct Process {
    pub pid: u64,
    pub name: Option<Rc<str>>,
    data_stack: Vec<u8>,
    call_stack: Vec<ProcessStack>,
    state: ProcessState,
    note: Note,
    output: Option<String>,
    play: bool
}

impl Process {
    pub fn new(pid: u64, name: Option<Rc<str>>, prog: Rc<Prog>) -> Process {
        let st = ProcessStack { memory: prog,
                                pc: PC(0),
                                dir: Dir::R };
        let mut stvec = Vec::new();
        stvec.push(st);
        Process { pid: pid,
                  name: name,
                  data_stack: Vec::new(),
                  call_stack: stvec,
                  state: ProcessState::Running(false),
                  note: Note::default(),
                  output: None,
                  play: false }
    }

    pub fn is_running(&self) -> bool {
        match self.state {
            ProcessState::Running(_) => true,
            _ => false
        }
    }

    pub fn state(&self) -> &ProcessState {
        &self.state
    }

    pub fn set_state(&mut self, st: ProcessState) {
        self.state = st
    }

    pub fn top(&self) -> Option<&ProcessStack> {
        let i = self.call_stack.len();
        if i == 0 {
            return None
        }

        self.call_stack.get(i - 1)
    }

    pub fn call_stack(&self) -> &Vec<ProcessStack> {
        &self.call_stack
    }

    pub fn top_mut(&mut self) -> Option<&mut ProcessStack> {
        let i = self.call_stack.len();
        if i == 0 {
            return None
        }

        self.call_stack.get_mut(i - 1)
    }

    pub fn call(&mut self, prog: Rc<Prog>, pc: PC, dir: Dir) {
        self.call_stack.push(
            ProcessStack { memory : prog,
                           pc: pc,
                           dir: dir });
    }

    pub fn r#return(&mut self) {
        self.call_stack.pop();
        if self.call_stack.len() == 0 {
            self.set_state(ProcessState::Finished);
        }
    }

    pub fn dir(&self) -> Option<Dir> {
        let top = self.top()?;
        Some(top.dir)
    }

    pub fn push(&mut self, i: u8) {
        self.data_stack.push(i)
    }

    pub fn pop(&mut self) -> Option<u8> {
        self.data_stack.pop()
    }

    pub fn null(&self) -> bool {
        self.data_stack.is_empty()
    }

    pub fn die(&mut self, reason: CrashReason) {
        self.state = ProcessState::Crashed(reason)
    }

    pub fn kill(&mut self) -> ProcessState {
        let mut prev = ProcessState::Killed;
        mem::swap(&mut prev, &mut self.state);
        prev
    }

    pub fn set_direction(&mut self, dir: Dir) {
        self.top_mut().map(|top| top.dir = dir);
    }

    pub fn step(&mut self) {
        match self.top_mut() {
            None => self.state = ProcessState::Finished,
            Some(top) => {
                let PC(i) = top.pc;
                let w = top.memory.cols();
                let h = top.memory.rows();
                match top.dir {
                    Dir::L => {
                        if i % w == 0 {
                            self.die(CrashReason::OutOfBounds(None));
                            return;
                        }
                        top.pc = PC(i - 1);
                    },
                    Dir::R => {
                        if i % w == (w - 1) {
                            self.die(CrashReason::OutOfBounds(None));
                            return;
                        }
                        top.pc = PC(i + 1);
                    }
                    Dir::U => {
                        if i / w == 0 {
                            self.die(CrashReason::OutOfBounds(None));
                            return;
                        }
                        top.pc = PC(i - w);
                    },
                    Dir::D => {
                        if i / w == h - 1 {
                            self.die(CrashReason::OutOfBounds(None));
                            return;
                        }
                        top.pc = PC(i + w);
                    },
                }
            }
        }
    }

    pub fn trap(&mut self, sys: Syscall) {
        self.set_state(ProcessState::Trap(sys));
    }

    pub fn resume(&mut self, push: Option<u8>) {
        match self.state {
            ProcessState::Trap(_) => { },
            _ => return
        }
        match push {
            None => {},
            Some(c) => self.data_stack.push(c)
        };
        self.set_state(ProcessState::Running(false));
    }

    pub fn apply(&mut self, op: &Op) {
        let f = &op.function;
        f(self)
    }

    pub fn peek(&self) -> Option<u8> {
        let top = self.top()?;
        let PC(i) = top.pc;
        Some(top.memory.data[i])
    }

    pub fn fork(&self, newpid: u64) -> Self {
        let mut new = self.clone();
        new.pid = newpid;
        return new
    }

    pub fn set_note(&mut self, note: Note) {
        self.note = note;
    }

    pub fn get_note(&self) -> &Note {
        &self.note
    }

    pub fn get_mut_note(&mut self) -> &mut Note {
        &mut self.note
    }

    pub fn clear_output(&mut self) {
        self.output = None;
        self.play = false;
    }

    pub fn set_output(&mut self, v: String) {
        self.output = Some(v)
    }

    pub fn get_output(&self) -> Option<String> {
        self.output.clone()
    }

    pub fn set_play(&mut self) {
        self.play = true;
    }

    pub fn get_played_note(&self) -> Option<Note> {
        if self.play {
            Some(self.note)
        } else {
            None
        }
    }

    pub fn data_stack_size(&self) -> usize {
        self.data_stack.len()
    }

    pub fn call_stack_size(&self) -> usize {
        self.call_stack.len()
    }
}

#[derive(Clone)]
pub struct Op {
    pub function: Rc<dyn Fn(&mut Process)>,
    pub opcode: u8,
    pub name: String,
    pub description: String
}

impl Op {
    pub fn new<S,T>(f: Rc<dyn Fn(&mut Process)>, opcode: u8, name: S,
               description: T) -> Self
        where S: ToString, T: ToString
    {
        Op {
            function: f,
            opcode: opcode,
            name: name.to_string(),
            description: description.to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_prog() {
        let pr = Prog::parse("12345\n\
                              67890\n\
                              a").unwrap();
        assert_eq!(pr.cols(), 5);
        assert_eq!(pr.rows(), 3);
        let v = vec![49,50,51,52,53,54,55,56,57,48,97,32,32,32,32];
        assert_eq!(pr.data, v);
        assert_eq!(pr.lookup(PC(0)), 49);
        assert_eq!(pr.lookup(PC(6)), 55);
    }

    #[test]
    fn bad_prog() {
        use ParseError::*;
        assert_eq!(Prog::parse(""), Err(Empty));
        assert_eq!(Prog::parse("\n\n\n\n\n"), Err(Empty));

        let mut long_line = String::with_capacity(512);
        let mut too_many = String::with_capacity(512);

        for _i in 0..256 {
            long_line.push_str("a");
            too_many.push_str("a\n");
        }

        assert_eq!(Prog::parse(&long_line), Err(LineTooLong));
        assert_eq!(Prog::parse(&too_many), Err(TooManyLines));
    }

}
