
use rand::Rng;
use std::rc::Rc;
use arr_macro::arr;

use super::process::{Process, Prog, ProcessState, Syscall, Dir, Op, PC};

macro_rules! pop {
    ($proc : ident) => {
        match $proc.pop() {
            Some(u) => u,
            None => {
                $proc.die("Pop from empty stack.");
                return
            }
        }
    }
}

macro_rules! make_op {
    ($fn : expr) => { Op::new(Rc::new($fn)) }
}

pub struct OpSet([Option<Op>; 256]);

impl OpSet {
    pub fn new() -> OpSet {
        let mut ops : [Option<Op>; 256] = arr![None;256];
        ops[32] = Some(make_op!(noop)); // Space
        ops[60] = Some(set_direction(Dir::L)); // >
        ops[62] = Some(set_direction(Dir::R)); // <
        ops[94] = Some(set_direction(Dir::U)); // ^
        ops[118] = Some(set_direction(Dir::D)); // v
        ops[63] = Some(make_op!(rand_direction)); // ?
        ops[59] = Some(make_op!(r#return)); // ;
        ops[64] = Some(make_op!(quit)); // @

        for i in 0..=9 { // 0 - 9
            ops[i as usize + 48] = Some(push_int(i));
        }
        for i in 0..=5 { // A - F
            ops[i as usize + 65] = Some(push_int(10 + i));
        }
        ops[104] = Some(make_op!(hex_byte)); // h

        ops[37] = Some(make_op!(r#mod)); // %
        ops[42] = Some(make_op!(mul)); // *
        ops[43] = Some(make_op!(add)); // +
        ops[45] = Some(make_op!(sub)); // -
        ops[47] = Some(make_op!(div)); // /

        ops[46] = Some(make_op!(send)); // .
        ops[126] = Some(make_op!(receive)); // ~
        ops[38] = Some(make_op!(print_byte)); // &
        ops[44] = Some(make_op!(print_char)); // ,

        ops[102] = Some(make_op!(fork)); // f
        ops[115] = Some(make_op!(sleep)); // s

        OpSet(ops)
    }

    pub fn apply_to(&self, proc: &mut Process) {
        let OpSet(ops) = self;
        let c = match proc.peek() {
            None => return,
            Some(c) => c
        };
        match &ops[c as usize] {
            None => { proc.die("Unknown op"); }
            Some(op) => proc.apply(op)
        }
    }

}

fn noop(proc: &mut Process) {
    proc.step()
}

fn push_int(i: u8) -> Op {
    let push_i = move |proc: &mut Process| {
        proc.push(i);
        proc.step()
    };
    make_op!(push_i)
}

fn hex_byte(proc: &mut Process) {
    let msb = pop!(proc);
    let lsb = pop!(proc);
    proc.push((msb << 4) + lsb);
    proc.step();
}

fn set_direction(dir: Dir) -> Op {
    let set_dir = move |proc: &mut Process| {
        proc.set_direction(dir);
        proc.step()
    };
    make_op!(set_dir)
}

fn rand_direction(proc: &mut Process) {
    let mut rng = rand::thread_rng();
    let dir = match rng.gen_range(0,4) {
        0 => Dir::L,
        1 => Dir::R,
        2 => Dir::U,
        3 => Dir::D,
        _ => panic!("Random number out of range [0,4)")
    };
    proc.set_direction(dir);
    proc.step();
}

fn sleep(proc: &mut Process) {
    let beats = pop!(proc);
    proc.trap(Syscall::Sleep(beats));
}
 
fn r#return(proc: &mut Process) {
    proc.r#return();
}

fn quit(proc: &mut Process) {
    proc.set_state(ProcessState::Finished);
}

fn add(proc: &mut Process) {
    let x = pop!(proc);
    let y = pop!(proc);
    proc.push(x + y);
    proc.step();
}

fn sub(proc: &mut Process) {
    let x = pop!(proc);
    let y = pop!(proc);
    proc.push(y - x);
    proc.step();
}

fn mul(proc: &mut Process) {
    let x = pop!(proc);
    let y = pop!(proc);
    proc.push(x * y);
    proc.step();
}

fn div(proc: &mut Process) {
    let x = pop!(proc);
    let y = pop!(proc);
    proc.push(y / x);
    proc.step();
}

fn r#mod(proc: &mut Process) {
    let x = pop!(proc);
    let y = pop!(proc);
    proc.push(y % x);
    proc.step();
}

fn fork(proc: &mut Process) {
    proc.trap(Syscall::Fork)
}

fn send(proc: &mut Process) {
    let ch = pop!(proc); // channel
    let c = pop!(proc); // value
    proc.trap(Syscall::Send(ch, c));
}

fn receive(proc: &mut Process) {
    let ch = pop!(proc);
    proc.trap(Syscall::Receive(ch));
}

fn print_byte(proc: &mut Process) {
    let c = pop!(proc);
    proc.trap(Syscall::PrintNum(c));
}

fn print_char(proc: &mut Process) {
    let c = pop!(proc);
    proc.trap(Syscall::PrintChar(c));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn demo_proc_1() -> Process {
        Process::new(1,
            Rc::new(Prog::parse(">    @\n\
                                 > 1 2^\n\
                                 >45+ ^\n\
                                 >95- ^\n\
                                 >35* ^\n\
                                 >C4/ ^\n\
                                 >B7% ^").expect("Bad test program.")))
    }

    #[test]
    fn test_noop() {
        let mut proc = demo_proc_1();
        proc.top_mut().map(|t| t.pc = PC(1));
        let ops = OpSet::new();
        ops.apply_to(&mut proc);
        let PC(i) = proc.top().expect("Empty top").pc;
        assert!(i == 2, "PC != 2");
        assert!(proc.state() == ProcessState::Running(false),
                "Process is not running.");

        // Rest of program plays out.
        for _ in 1..10 {
            ops.apply_to(&mut proc);
        }
        assert!(proc.state() == ProcessState::Finished,
                "Process is not running.");
    }

    #[test]
    fn test_math_ops() {
        let mut results = Vec::new();
        let ops = OpSet::new();
        for i in 2..7 {
            let mut proc = demo_proc_1();
            proc.top_mut().map(|t| t.pc = PC(i * 6));
            for _ in 1..10 {
                ops.apply_to(&mut proc);
            }
            results.push(pop!(proc));
        }
        assert!(results == vec![9,4,15,3,4],
                "Unexpected results {:?}.", results);
    }

}
