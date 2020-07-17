
use noisefunge::befunge::{OpSet, CharMap};

fn main() {
    let charmap = CharMap::default();
    let opset = OpSet::default();

    for i in 0..=255 {
        let op = match opset.lookup(i) {
            None => continue,
            Some(op) => op
        };
        println!("{:2X} | {:1} | {:11} | {}", op.opcode, charmap[op.opcode],
                 op.name, op.description);
    }
}
