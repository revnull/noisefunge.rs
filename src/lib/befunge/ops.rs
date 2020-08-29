use rand::Rng;
use std::cmp;
use std::rc::Rc;
use arr_macro::arr;

use super::process::{Process, ProcessState, Syscall, Dir, Op, Note,
                     CrashReason};

macro_rules! pop {
    ($proc : ident) => {
        match $proc.pop() {
            Some(u) => u,
            None => {
                $proc.die(CrashReason::PopFromEmptyStack);
                return
            }
        }
    }
}

macro_rules! make_op {
    ($code: expr, $name: expr, $desc: expr, $func: expr) => {
        Op::new(Rc::new($func), $code, $name, $desc)
    }
}

pub struct OpSet([Option<Op>; 256]);

impl OpSet {
    pub fn new() -> Self {
        OpSet(arr![None; 256])
    }

    pub fn default() -> Self {
        let mut ops = OpSet::new();
        ops.insert_safe(make_op!(32, "Noop", "No operation", noop));

        ops.insert_safe(set_direction(Dir::L));
        ops.insert_safe(set_direction(Dir::R));
        ops.insert_safe(set_direction(Dir::U));
        ops.insert_safe(set_direction(Dir::D));
        ops.insert_safe(
            make_op!(63, "Rand(Dir)", "Change to random direction.",
                     rand_direction));
        ops.insert_safe(
            make_op!(82, "Rand(Byte)",
                     "Pop x and y. Push a number between (inclusive)",
                     rand_range));

        ops.insert_safe(make_op!(34, "Quote", "Start/Stop quote mode", quote));

        ops.insert_safe(make_op!(64, "Quit", "Terminate the program", quit));

        for i in 0..=15 { // 0 - F
            ops.insert_safe(push_int(i));
        }
        ops.insert_safe(
            make_op!(104, "Hex Byte", "Pop x and y. Push (x*16)+y.",
                     hex_byte));
        ops.insert_safe(
            make_op!(110, "Note", "Pop o and x. Push (o*12)+x.",
                     push_note));


        ops.insert_safe(
            make_op!(37, "Modulo", "Pop x and y. Push y % x.", r#mod));
        ops.insert_safe(
            make_op!(42, "Multiply", "Pop x and y. Push y * x.", mul));
        ops.insert_safe(
            make_op!(43, "Add", "Pop x and y. Push y + x.", add));
        ops.insert_safe(
            make_op!(45, "Subtract", "Pop x and y. Push y - x.", sub));
        ops.insert_safe(
            make_op!(47, "Divide", "Pop x and y. Push y / x.", div));

        ops.insert_safe(
            make_op!(33, "Not", "Pop x. If x = 0, push 1, or else 0.", not));
        ops.insert_safe(
            make_op!(61, "Equal",
                     "Pop x and y. If y = x, push 1, or else 0.", eq));
        ops.insert_safe(
            make_op!(96, "Greater",
                     "Pop x and y. If y > x, push 1, or else 0.", gt));
        ops.insert_safe(
            make_op!(95, "Cond(H)",
                     "Pop x. If x is 0, go right, or else left.", cond_h));
        ops.insert_safe(
            make_op!(124, "Cond(V)",
                     "Pop x. If x is 0, go down, or else up.", cond_v));
        ops.insert_safe(
            make_op!(39, "CondJump", "Pop x. If x != 0, jump.", cond_jump));

        ops.insert_safe(
            make_op!(46, "Send", "Pop c and x. Send c to channel x.", send));
        ops.insert_safe(
            make_op!(126, "Receive",
                     "Pop c. Read from channel c and push result.", receive));
        ops.insert_safe(
            make_op!(38, "Print(Byte)",
                     "Pop x. Print x as a hex byte.", print_byte));
        ops.insert_safe(
            make_op!(44, "Print(Char)",
                     "Pop x. Print x as character.", print_char));

        ops.insert_safe(
            make_op!(102, "Fork",
                     "Fork thread. Push 1 for child, 0 for parent.", fork));
        ops.insert_safe(
            make_op!(115, "Sleep", "Pop x. Sleep for x subbeats.", sleep));
        ops.insert_safe(
            make_op!(113, "Quantize", "Sleep until the next full beat.",
                     quantize));
        ops.insert_safe(
            make_op!(81, "QuantizeN",
                     "Pop x. Sleep until full beat divisible by x.",
                     quantize_n));

        ops.insert_safe(
            make_op!(36, "Chomp", "Discard value at top of stack.", chomp));
        ops.insert_safe(
            make_op!(58, "Dup", "Duplicate value at top of stack.", dup));
        ops.insert_safe(
            make_op!(92, "Swap", "Pop x and y. Push x and y.", swap));
        ops.insert_safe(
            make_op!(78, "Null?", "Push 1 if stack is empty, otherwise 0",
                     null));

        ops.insert_safe(
            make_op!(91, "Defop", "Define user opcode.", defop));
        ops.insert_safe(
            make_op!(93, "Return", "Return from user opcode.", r#return));

        ops.insert_safe(
            make_op!(101, "Execute", "Pop x. Execute it as an opcode.",
                     execute));
        ops.insert_safe(
            make_op!(99, "Call", "Pop y and x. Call opcode at (y, x).", call));
        ops.insert_safe(
            make_op!(71, "Goto", "Pop y and x. Go to position (y, x).", goto));
        ops.insert_safe(
            make_op!(35, "Jump", "Skip over next position.", jump));
        ops.insert_safe(
            make_op!(112, "Put", "Pop y, x, and c. Write c to position (y, x)",
                     put));
        ops.insert_safe(
            make_op!(103, "Get",
                     "Pop y and x. Push value from (y, x) onto stack.", get));
        ops.insert_safe(
            make_op!(59, "Drop",
                     "Pop c and write it to the current position.", drop));

        ops.insert_safe(
            make_op!(90, "Play", "Play note in note buffer.", play));
        ops.insert_safe(
            make_op!(122, "Write(Note)",
                     "Pop dur, vel, pch, cha. Write note buffer.", writebuf));
        ops.insert_safe(
            make_op!(117, "Write(Dur)",
                     "Pop x. Write x as note buffer duration.", writebuf_dur));
        ops.insert_safe(
            make_op!(119, "Write(Vel)",
                     "Pop x. Write x as note buffer velocity.", writebuf_vel));
        ops.insert_safe(
            make_op!(120, "Write(Pch)",
                     "Pop x. Write x as note buffer pitch.", writebuf_pch));
        ops.insert_safe(
            make_op!(121, "Write(Cha)",
                     "Pop x. Write x as note buffer channel.", writebuf_cha));
        ops.insert_safe(
            make_op!(85, "Read(Dur)", "Push note buffer duration.",
                     readbuf_dur));
        ops.insert_safe(
            make_op!(87, "Read(Vel)", "Push note buffer velocity.",
                     readbuf_vel));
        ops.insert_safe(
            make_op!(88, "Read(Pch)", "Push note buffer pitch.",
                     readbuf_pch));
        ops.insert_safe(
            make_op!(89, "Read(Cha)", "Push note buffer channel.",
                     readbuf_cha));

        ops
    }

    pub fn insert(&mut self, op: Op) {
        let i = op.opcode as usize;
        self.0[i] = Some(op)
    }

    fn insert_safe(&mut self, op: Op) {
        if self.0[op.opcode as usize].is_some() {
            panic!(format!("Duplicate opcode for {:X}", op.opcode));
        }
        self.insert(op)
    }

    pub fn apply_to(&self, proc: &mut Process, o: Option<u8>) {
        let OpSet(ops) = self;
        let c = match o.or_else(|| proc.peek()) {
            None => return,
            Some(c) => c
        };
        match &ops[c as usize] {
            None => { proc.die(CrashReason::InvalidOpcode(c)); }
            Some(op) => proc.apply(op)
        }
    }

    pub fn defop(&mut self, c: u8, op: Op) {
        self.0[c as usize] = Some(op);
    }

    pub fn lookup(&self, c: u8) -> Option<&Op> {
        self.0[c as usize].as_ref()
    }

}

fn noop(_proc: &mut Process) {

}

fn quote(proc: &mut Process) {
    proc.set_state(ProcessState::Running(true));
}

fn push_int(i: u8) -> Op {
    let opcode = if i < 10 { 48 + i } else { 65 + i - 10 };

    let push_i = move |proc: &mut Process| {
        proc.push(i);
    };
    make_op!(opcode, format!("{:X}", i), format!("Push {} onto the stack", i),
             push_i)
}

fn hex_byte(proc: &mut Process) {
    let msb = pop!(proc);
    let lsb = pop!(proc);
    proc.push((msb << 4) + lsb);
}

fn push_note(proc: &mut Process) {
    let oct = pop!(proc);
    let note = pop!(proc);
    proc.push((oct * 12) + note);
}
fn set_direction(dir: Dir) -> Op {
    let (opcode, name) = match dir {
        Dir::U => (94, "Up"),
        Dir::D => (118, "Down"),
        Dir::L => (60, "Left"),
        Dir::R => (62, "Right"),
    };
    let set_dir = move |proc: &mut Process| {
        proc.set_direction(dir);
    };
    make_op!(opcode, name, format!("Change direction to {}", name), set_dir)
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

fn rand_range(proc: &mut Process) {
    let mut rng = rand::thread_rng();
    let x = pop!(proc);
    let y = pop!(proc);
    let rmin = cmp::min(x, y) as u16;
    let rmax = cmp::max(x, y) as u16 + 1;
    let z = rng.gen_range(rmin, rmax);
    proc.push(z as u8);
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
        proc.die(CrashReason::InvalidQuantize);
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
        None => proc.die(CrashReason::OutOfBounds(Some(99))),
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
        None => proc.die(CrashReason::OutOfBounds(Some(71))),
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

fn cond_jump(proc: &mut Process) {
    let x = pop!(proc);
    if x != 0 {
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

fn null(proc: &mut Process) {
    if proc.null() {
        proc.push(1);
    } else {
        proc.push(0);
    }
}

fn drop(proc: &mut Process) {
    let c = pop!(proc);

    let top = proc.top_mut().unwrap();
    Rc::make_mut(&mut top.memory).update(top.pc, c);
}

fn put(proc: &mut Process) {
    let y = pop!(proc) as usize;
    let x = pop!(proc) as usize;
    let c = pop!(proc);

    let top = proc.top_mut().unwrap();
    let pc = match top.memory.xy_to_pc(x, y) {
        Some(pc) => pc,
        None => {
            proc.die(CrashReason::OutOfBounds(Some(112)));
            return
        }
    };
    Rc::make_mut(&mut top.memory).update(pc, c);
}

fn get(proc: &mut Process) {
    let y = pop!(proc) as usize;
    let x = pop!(proc) as usize;

    let top = proc.top().unwrap();
    let pc = match top.memory.xy_to_pc(x, y) {
        Some(pc) => pc,
        None => {
            proc.die(CrashReason::OutOfBounds(Some(103)));
            return
        }
    };
    let c = top.memory.lookup(pc);
    proc.push(c);
    
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

