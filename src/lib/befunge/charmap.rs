
use std::ops::Index;

const DEFAULT_CHARMAP: &str = "\u{22a5}\u{2200}\u{2202}\u{2204}\u{2205}\u{2208}\u{220b}\u{220f}\u{2211}\u{2213}\u{2218}\u{000b}\u{000c}\u{000d}\u{000e}\u{000f}\u{0010}\u{0011}\u{0012}\u{0013}\u{0014}\u{0015}\u{0016}\u{0017}\u{0018}\u{0019}\u{001a}\u{001b}\u{001c}\u{001d}\u{001e}\u{001f} !\"#$%&'()*+,-./0123456789:;\u{2b05}=\u{27a1}?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]\u{2b06}_`abcdefghijklmnopqrstu\u{2b07}wxyz{|}~\u{007f}\u{0080}\u{0081}\u{0082}\u{0083}\u{0084}\u{0085}\u{0086}\u{0087}\u{0088}\u{0089}\u{008a}\u{008b}\u{008c}\u{008d}\u{008e}\u{008f}\u{0090}\u{0091}\u{0092}\u{0093}\u{0094}\u{0095}\u{0096}\u{0097}\u{0098}\u{0099}\u{009a}\u{009b}\u{009c}\u{009d}\u{009e}\u{009f}\u{00a0}\u{00a1}\u{00a2}\u{00a3}\u{00a4}\u{00a5}\u{00a6}\u{00a7}\u{00a8}\u{00a9}\u{00aa}\u{00ab}\u{00ac}\u{00ad}\u{00ae}\u{00af}\u{00b0}\u{00b1}\u{00b2}\u{00b3}\u{00b4}\u{00b5}\u{00b6}\u{00b7}\u{00b8}\u{00b9}\u{00ba}\u{00bb}\u{00bc}\u{00bd}\u{00be}\u{00bf}\u{00c0}\u{00c1}\u{00c2}\u{00c3}\u{00c4}\u{00c5}\u{00c6}\u{00c7}\u{00c8}\u{00c9}\u{00ca}\u{00cb}\u{00cc}\u{00cd}\u{00ce}\u{00cf}\u{00d0}\u{00d1}\u{00d2}\u{00d3}\u{00d4}\u{00d5}\u{00d6}\u{00d7}\u{00d8}\u{00d9}\u{00da}\u{00db}\u{00dc}\u{00dd}\u{00de}\u{00df}\u{00e0}\u{00e1}\u{00e2}\u{00e3}\u{00e4}\u{00e5}\u{00e6}\u{00e7}\u{00e8}\u{00e9}\u{00ea}\u{00eb}\u{00ec}\u{00ed}\u{00ee}\u{00ef}\u{00f0}\u{00f1}\u{00f2}\u{00f3}\u{00f4}\u{00f5}\u{00f6}\u{00f7}\u{00f8}\u{00f9}\u{00fa}\u{00fb}\u{00fc}\u{00fd}\u{00fe}\u{00ff}";

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
                assert_eq!(cm[i], '\u{2b07}');
                continue;
            }
            assert_eq!(cm[i], i as char);
        }
        assert_eq!(cm['>' as u8], '\u{27a1}');
    }
}

