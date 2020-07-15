use rand::Rng;
use std::rc::Rc;
use arr_macro::arr;

use super::process::{Process, Prog, ProcessState, Syscall, Dir, Op, PC, Note};

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

        ops[34] = Some(make_op!(quote)); // "

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
        ops[33] = Some(make_op!(not)); // !
        ops[61] = Some(make_op!(eq)); // !
        ops[96] = Some(make_op!(gt)); // `

        ops[95] = Some(make_op!(cond_h)); // _
        ops[124] = Some(make_op!(cond_v)); // |

        ops[46] = Some(make_op!(send)); // .
        ops[126] = Some(make_op!(receive)); // ~
        ops[38] = Some(make_op!(print_byte)); // &
        ops[44] = Some(make_op!(print_char)); // ,

        ops[102] = Some(make_op!(fork)); // f
        ops[115] = Some(make_op!(sleep)); // s
        ops[113] = Some(make_op!(quantize)); // q
        ops[81] = Some(make_op!(quantize_n)); // Q

        ops[36] = Some(make_op!(chomp)); // $
        ops[58] = Some(make_op!(dup)); // :
        ops[92] = Some(make_op!(swap)); // \

        ops[91] = Some(make_op!(defop)); // [
        ops[93] = Some(make_op!(r#return)); // ]
        ops[99] = Some(make_op!(call)); // c
        ops[101] = Some(make_op!(execute)); // e
        ops[103] = Some(make_op!(goto)); // g
        ops[35] = Some(make_op!(jump)); // #

        ops[112] = Some(make_op!(put)); // p
        ops[103] = Some(make_op!(get)); // g

        ops[90] = Some(make_op!(play)); // Z
        ops[122] = Some(make_op!(writebuf)); // z
        ops[117] = Some(make_op!(writebuf_dur)); // u
        ops[119] = Some(make_op!(writebuf_vel)); // w
        ops[120] = Some(make_op!(writebuf_pch)); // x
        ops[121] = Some(make_op!(writebuf_cha)); // y
        ops[85] = Some(make_op!(readbuf_dur)); // U
        ops[87] = Some(make_op!(readbuf_vel)); // W
        ops[88] = Some(make_op!(readbuf_pch)); // X
        ops[89] = Some(make_op!(readbuf_cha)); // Y

        OpSet(ops)
    }

    pub fn apply_to(&self, proc: &mut Process, o: Option<u8>) {
        let OpSet(ops) = self;
        let c = match o.or_else(|| proc.peek()) {
            None => return,
            Some(c) => c
        };
        match &ops[c as usize] {
            None => { proc.die("Unknown op"); }
            Some(op) => proc.apply(op)
        }
    }

    pub fn defop(&mut self, c: u8, op: Op) {
        self.0[c as usize] = Some(op);
    }

}

fn noop(proc: &mut Process) {

}

fn quote(proc: &mut Process) {
    proc.set_state(ProcessState::Running(true));
}

fn push_int(i: u8) -> Op {
    let push_i = move |proc: &mut Process| {
        proc.push(i);
    };
    make_op!(push_i)
}

fn hex_byte(proc: &mut Process) {
    let msb = pop!(proc);
    let lsb = pop!(proc);
    proc.push((msb << 4) + lsb);
}

fn set_direction(dir: Dir) -> Op {
    let set_dir = move |proc: &mut Process| {
        proc.set_direction(dir);
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
}

fn sleep(proc: &mut Process) {
    let beats = pop!(proc);
    proc.trap(Syscall::Sleep(beats as u32));
}
 
fn quantize(proc: &mut Process) {
    proc.trap(Syscall::Quantize(1));
}
 
fn quantize_n(proc: &mut Process) {
    let q = pop!(proc);
    if q == 0 {
        proc.die("Can't quantize on 0 beats.");
    } else {
        proc.trap(Syscall::Quantize(q));
    }
}
 
fn defop(proc: &mut Process) {
    let op = pop!(proc);
    proc.trap(Syscall::Defop(op));
}

fn r#return(proc: &mut Process) {
    proc.r#return();
}

fn execute(proc: &mut Process) {
    let c = pop!(proc);
    proc.trap(Syscall::Call(c));
}

fn call(proc: &mut Process) {
    let y = pop!(proc) as usize;
    let x = pop!(proc) as usize;
    let prog = &proc.top().unwrap().memory;
    match prog.xy_to_pc(x, y).map(|pc| prog.lookup(pc)) {
        Some(c) => proc.trap(Syscall::Call(c)),
        None => proc.die("Call exceeded bounds"),
    }
}

fn goto(proc: &mut Process) {
    let y = pop!(proc) as usize;
    let x = pop!(proc) as usize;
    let mut top = proc.top_mut().unwrap();
    match top.memory.xy_to_pc(x, y) {
        Some(pc) => {
            top.pc = pc;
            proc.trap(Syscall::Pause);
        }
        None => proc.die("Call exceeded bounds"),
    }
}

fn quit(proc: &mut Process) {
    proc.set_state(ProcessState::Finished);
}

fn add(proc: &mut Process) {
    let x = pop!(proc);
    let y = pop!(proc);
    proc.push(x.wrapping_add(y));
}

fn sub(proc: &mut Process) {
    let x = pop!(proc);
    let y = pop!(proc);
    proc.push(y.wrapping_sub(x));
}

fn mul(proc: &mut Process) {
    let x = pop!(proc);
    let y = pop!(proc);
    proc.push(x.wrapping_mul(y));
}

fn div(proc: &mut Process) {
    let x = pop!(proc);
    let y = pop!(proc);
    proc.push(y.wrapping_div(x));
}

fn r#mod(proc: &mut Process) {
    let x = pop!(proc);
    let y = pop!(proc);
    proc.push(y % x);
}

fn not(proc: &mut Process) {
    let x = pop!(proc);
    if x == 0 {
        proc.push(1);
    } else {
        proc.push(0);
    }
}

fn eq(proc: &mut Process) {
    let x = pop!(proc);
    let y = pop!(proc);

    if x == y {
        proc.push(1);
    } else {
        proc.push(0);
    }
}

fn gt(proc: &mut Process) {
    let x = pop!(proc);
    let y = pop!(proc);

    if y > x {
        proc.push(1);
    } else {
        proc.push(0);
    }
}

fn jump(proc: &mut Process) {
    proc.step();
}

fn condjump(proc: &mut Process) {
    let x = pop!(proc);
    if x == 0 {
        proc.step();
    }
}

fn cond_h(proc: &mut Process) {
    let x = pop!(proc);

    if x == 0 {
        proc.set_direction(Dir::R);
    } else {
        proc.set_direction(Dir::L);
    }
}

fn cond_v(proc: &mut Process) {
    let x = pop!(proc);

    if x == 0 {
        proc.set_direction(Dir::D);
    } else {
        proc.set_direction(Dir::U);
    }
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

fn dup(proc: &mut Process) {
    let c = pop!(proc);
    proc.push(c);
    proc.push(c);
}

fn chomp(proc: &mut Process) {
    pop!(proc);
}

fn swap(proc: &mut Process) {
    let c = pop!(proc);
    let d = pop!(proc);
    proc.push(c);
    proc.push(d);
}

fn put(proc: &mut Process) {
    let y = pop!(proc) as usize;
    let x = pop!(proc) as usize;
    let c = pop!(proc);

    let mut top = proc.top_mut().unwrap();
    match top.memory.xy_to_pc(x, y) {
        Some(pc) => {
            Rc::make_mut(&mut top.memory).update(pc, c);
        },
        None => proc.die("Put outside of bounds."),
    }
}

fn get(proc: &mut Process) {
    let y = pop!(proc) as usize;
    let x = pop!(proc) as usize;

    let top = proc.top().unwrap();
    match top.memory.xy_to_pc(x, y) {
        Some(pc) => {
            proc.push(top.memory.lookup(pc));
        },
        None => proc.die("Get outside of bounds."),
    }
    
}

fn writebuf(proc: &mut Process) {
    let dur = pop!(proc);
    let vel = pop!(proc);
    let pch = pop!(proc);
    let cha = pop!(proc);

    proc.set_note(Note { pch: pch, vel: vel, cha: cha, dur: dur });
}

fn writebuf_dur(proc: &mut Process) {
    let dur = pop!(proc);
    proc.get_mut_note().dur = dur;
}

fn writebuf_vel(proc: &mut Process) {
    let vel = pop!(proc);
    proc.get_mut_note().vel = vel;
}

fn writebuf_pch(proc: &mut Process) {
    let pch = pop!(proc);
    proc.get_mut_note().pch = pch;
}

fn writebuf_cha(proc: &mut Process) {
    let cha = pop!(proc);
    proc.get_mut_note().cha = cha;
}

fn readbuf_dur(proc: &mut Process) {
    proc.push(proc.get_note().dur);
}

fn readbuf_vel(proc: &mut Process) {
    proc.push(proc.get_note().vel);
}

fn readbuf_pch(proc: &mut Process) {
    proc.push(proc.get_note().pch);
}

fn readbuf_cha(proc: &mut Process) {
    proc.push(proc.get_note().cha);
}

fn play(proc: &mut Process) {
    proc.trap(Syscall::Play(*proc.get_note()));
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
        ops.apply_to(&mut proc, None);
        proc.step();
        let PC(i) = proc.top().expect("Empty top").pc;
        assert!(i == 2, "PC != 2");
        assert!(*proc.state() == ProcessState::Running(false),
                "Process is not running.");

        // Rest of program plays out.
        for _ in 1..5 {
            ops.apply_to(&mut proc, None);
            if proc.is_running() {
                proc.step();
            }
        }
        assert!(*proc.state() == ProcessState::Finished,
                format!("Process is not finished: {:?}", proc.state()));
    }

    #[test]
    fn test_math_ops() {
        let mut results = Vec::new();
        let ops = OpSet::new();
        for i in 2..7 {
            let mut proc = demo_proc_1();
            proc.top_mut().map(|t| t.pc = PC(i * 6));
            for _ in 1..10 {
                ops.apply_to(&mut proc, None);
            }
            results.push(pop!(proc));
        }
        assert!(results == vec![9,4,15,3,4],
                "Unexpected results {:?}.", results);
    }

}
