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

use std::ops::Index;

const DEFAULT_CHARMAP: &str = "\u{22a5}\u{2200}\u{2202}\u{2204}\u{2205}\u{2208}\u{220b}\u{220f}\u{2211}\u{2213}\u{2218}\u{2219}\u{221d}\u{221e}\u{2220}\u{2229}\u{222a}\u{222b}\u{222c}\u{2237}\u{2239}\u{223b}\u{2241}\u{2244}\u{2246}\u{224d}\u{224e}\u{2251}\u{2252}\u{2256}\u{2257}\u{225a} !\"#$%&'()*+,-./0123456789:;\u{25c2}=\u{25b8}?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]\u{25b4}\u{2b0c}`abcdefghijklmnopqrstu\u{25be}wxyz{\u{2b0d}}~\u{2272}\u{2271}\u{2278}\u{2280}\u{2284}\u{2285}\u{2295}\u{2296}\u{2297}\u{2298}\u{229a}\u{229b}\u{229c}\u{229e}\u{229f}\u{22a0}\u{22a2}\u{22a3}\u{260e}\u{22b8}\u{267b}\u{22c2}\u{22c3}\u{22c4}\u{22c6}\u{22ce}\u{22cf}\u{22d0}\u{22d1}\u{22da}\u{22db}\u{22de}\u{22df}\u{22e2}\u{22e3}\u{22e4}\u{22e5}\u{22e6}\u{22e7}\u{22e8}\u{22e9}\u{22ef}\u{2301}\u{2306}\u{2311}\u{2312}\u{2313}\u{2314}\u{2315}\u{2102}\u{204b}\u{210d}\u{210e}\u{210f}\u{2115}\u{2117}\u{2119}\u{211a}\u{211d}\u{2122}\u{2124}\u{2126}\u{212b}\u{2148}\u{21af}\u{21b0}\u{21b1}\u{21b2}\u{21b3}\u{21b4}\u{21b5}\u{21b9}\u{21ba}\u{21bb}\u{2371}\u{2372}\u{2373}\u{2374}\u{2376}\u{2377}\u{2380}\u{2388}\u{238a}\u{2764}\u{2765}\u{2734}\u{27c5}\u{27c6}\u{27dc}\u{27e0}\u{27ea}\u{27eb}\u{27eb}\u{29fa}\u{29fb}\u{2a00}\u{2a6a}\u{2a6b}\u{2b12}\u{2b13}\u{2b14}\u{2b15}\u{2b16}\u{2b17}\u{2b18}\u{2b19}\u{266a}\u{2669}\u{269b}\u{262e}\u{25f0}\u{25f1}\u{25f2}\u{25f3}\u{260b}\u{260a}\u{2603}\u{2620}\u{2622}\u{2680}\u{2681}\u{2682}\u{2683}\u{2684}\u{2685}\u{2697}\u{2692}\u{2696}\u{26a0}";

pub struct CharMap(Vec<char>);

impl CharMap {
    pub fn new(chars: &str) -> Self {
        let vec : Vec<char> = chars.chars().collect();
        assert_eq!(vec.len(), 256);
        CharMap(vec)
    }

    pub fn default() -> Self {
        Self::new(DEFAULT_CHARMAP)
    }
}

impl Index<u8> for CharMap {
    type Output = char;

    fn index(&self, i: u8) -> &Self::Output {
        let CharMap(v) = self;
        &v[i as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn default_is_valid() {
        let cm = CharMap::default();
        for i in 65..=90 {
            assert_eq!(cm[i], i as char);
        }
        for i in 97..=122 {
            if i == 118 { // v is down arrow
                assert_eq!(cm[i], '\u{2bc6}');
                continue;
            }
            assert_eq!(cm[i], i as char);
        }
        assert_eq!(cm['>' as u8], '\u{2bc8}');

        let mut set = HashSet::with_capacity(256);
        for i in 0..=255 {
            let c = cm[i];
            if !set.insert(c) {
                panic!("Duplicate entry: {}", c);
            }
        }
    }
}

