
use rand::Rng;
use super::process::{Process, ProcessState, Syscall, Dir};

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

pub struct Op(Box<Fn(&mut Process)>);

struct OpSet([Option<Op>; 256]);

impl OpSet {
    fn new() -> OpSet {
        let mut ops : [Option<Op>; 256] = [
            // I guess some 3rd party crates solve this problem...
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
        ];
        ops[32] = Some(Op(Box::new(noop))); // Space
        ops[60] = Some(set_direction(Dir::L)); // >
        ops[62] = Some(set_direction(Dir::R)); // <
        ops[94] = Some(set_direction(Dir::U)); // ^
        ops[118] = Some(set_direction(Dir::D)); // v
        ops[63] = Some(Op(Box::new(rand_direction))); // ?
        ops[64] = Some(Op(Box::new(r#return))); // @

        for i in 0..=9 { // 0 - 9
            ops[i as usize + 48] = Some(push_int(i));
        }
        for i in 0..=5 { // A - F
            ops[i as usize + 65] = Some(push_int(10 + i));
        }

        ops[37] = Some(Op(Box::new(r#mod))); // %
        ops[42] = Some(Op(Box::new(mul))); // *
        ops[43] = Some(Op(Box::new(add))); // +
        ops[45] = Some(Op(Box::new(sub))); // -
        ops[47] = Some(Op(Box::new(div))); // /

        OpSet(ops)
    }
}

fn noop(proc: &mut Process) {
    proc.step()
}

fn push_int(i: u8) -> Op {
    let push_i = move |proc: &mut Process| {
        proc.push(i)
    };
    Op(Box::new(push_i))
}

fn set_direction(dir: Dir) -> Op {
    let set_dir = move |proc: &mut Process| {
        proc.set_direction(dir);
        proc.step()
    };
    Op(Box::new(set_dir))
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

