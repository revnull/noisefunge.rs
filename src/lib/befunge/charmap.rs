
use std::ops::Index;

const DEFAULT_CHARMAP: &str = "\u{22a5}\u{2200}\u{2202}\u{2204}\u{2205}\u{2208}\u{220b}\u{220f}\u{2211}\u{2213}\u{2218}\u{2219}\u{221d}\u{221e}\u{2221}\u{2229}\u{222a}\u{222b}\u{222f}\u{2237}\u{223e}\u{223f}\u{2241}\u{2244}\u{2246}\u{224d}\u{224e}\u{2251}\u{2252}\u{2256}\u{2257}\u{225a} !\"#$%&'()*+,-./0123456789:;\u{2bc7}=\u{2bc8}?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]\u{2bc5}_`abcdefghijklmnopqrstu\u{2bc6}wxyz{|}~\u{2272}\u{2271}\u{2278}\u{2280}\u{2284}\u{2285}\u{2295}\u{2296}\u{2297}\u{2298}\u{229a}\u{229b}\u{229e}\u{229e}\u{229f}\u{22a0}\u{22a2}\u{22a3}\u{22a4}\u{22b6}\u{22b7}\u{22b9}\u{22bb}\u{22bc}\u{22be}\u{22bf}\u{22c4}\u{22c6}\u{22c7}\u{22c8}\u{22c9}\u{22ca}\u{22cb}\u{22cc}\u{22d4}\u{22d5}\u{22de}\u{22df}\u{22e2}\u{22e3}\u{22ec}\u{22ed}\u{22ee}\u{22ef}\u{22f0}\u{22f1}\u{22f3}\u{22fa}\u{22ff}\u{2102}\u{2103}\u{2107}\u{2108}\u{2109}\u{210d}\u{210f}\u{2111}\u{2114}\u{2115}\u{211e}\u{2123}\u{2124}\u{2125}\u{2126}\u{2127}\u{2128}\u{2129}\u{212f}\u{2130}\u{2132}\u{2135}\u{213c}\u{213e}\u{213f}\u{2140}\u{2141}\u{2142}\u{2143}\u{2144}\u{214a}\u{214b}\u{214f}\u{27c0}\u{27c1}\u{27c3}\u{2734}\u{27c5}\u{27c6}\u{27cc}\u{27ce}\u{27cf}\u{27d0}\u{27d2}\u{27d3}\u{27d4}\u{27da}\u{27db}\u{27dc}\u{27df}\u{27e4}\u{27e5}\u{27e6}\u{27e7}\u{27ea}\u{27eb}\u{2991}\u{2992}\u{299d}\u{299e}\u{29b0}\u{29b1}\u{29b7}\u{29b8}\u{29b9}\u{29bc}\u{29be}\u{29bf}\u{29c4}\u{29c5}\u{29c9}\u{29cc}\u{29d0}\u{29d1}\u{29d2}\u{29d4}\u{29d5}\u{29d6}\u{29d7}\u{29fb}";

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
        assert_eq!(cm['>' as u8], '\u{27a1}');
    }
}

