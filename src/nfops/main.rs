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
