//! Pinyin helpers: numeric pinyin (`ni3 hao3`) to tone-marked display
//! (`nǐ hǎo`), plus tone sandhi helpers retained for future word-level audio
//! composition.
//!
//! The dataset stores tones as trailing digits so it stays easy to diff and
//! to map onto one clip per syllable if word composition returns.

/// Convert every whitespace-separated syllable in `pinyin` to tone marks.
///
/// Erhua syllables (`r5`/`r`) are appended to the previous syllable, so
/// `na3 r5` renders as `nǎr` and `yi1 hui4 r5` as `yī huìr`.
pub fn marks(pinyin: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    for syllable in pinyin.split_whitespace() {
        if (syllable == "r5" || syllable == "r") && !out.is_empty() {
            if let Some(last) = out.last_mut() {
                last.push('r');
                continue;
            }
        }
        out.push(syllable_marks(syllable));
    }
    out.join(" ")
}

fn syllable_marks(syllable: &str) -> String {
    let normalized = normalize(syllable);
    let (base, tone) = split_tone(&normalized);

    let Some(tone) = tone else {
        return base.to_string();
    };
    if !(1..=4).contains(&tone) {
        return base.to_string();
    }

    let mut chars: Vec<char> = base.chars().collect();
    if let Some(index) = mark_position(&chars) {
        chars[index] = add_tone(chars[index], tone);
    }
    chars.into_iter().collect()
}

/// Apply the sandhi rules that matter when composing word audio.
///
/// The current release plays one character clip at a time and does not call
/// this yet; it is retained for word-level clips and HSK 3.0 character groups.
/// Rules implemented: third-tone + third-tone -> second + third, `bu4` before
/// a fourth tone -> `bu2`, and `yi1` before a fourth tone -> `yi2` / before
/// tones 1-3 -> `yi4`.
#[allow(dead_code)]
pub fn sandhi(keys: &[String]) -> Vec<String> {
    let mut out = keys.to_vec();
    if out.len() < 2 {
        return out;
    }

    for index in 0..out.len() - 1 {
        let base = base_of(&out[index]).to_string();
        let tone = tone_of(&out[index]);
        let next_tone = tone_of(&out[index + 1]);

        if base == "bu" && tone == Some(4) && next_tone == Some(4) {
            out[index] = with_tone(&out[index], 2);
        }

        if base == "yi" {
            match next_tone {
                Some(4) => out[index] = with_tone(&out[index], 2),
                Some(1..=3) => out[index] = with_tone(&out[index], 4),
                _ => {},
            }
        }
    }

    for index in 0..out.len() - 1 {
        if tone_of(&out[index]) == Some(3) && tone_of(&out[index + 1]) == Some(3) {
            out[index] = with_tone(&out[index], 2);
        }
    }

    out
}

pub fn base_of(key: &str) -> &str {
    let mut chars = key.char_indices();
    if let Some((index, last)) = chars.next_back() {
        if last.is_ascii_digit() {
            return &key[..index];
        }
    }
    key
}

pub fn tone_of(key: &str) -> Option<u8> {
    key.chars()
        .last()
        .and_then(|c| c.to_digit(10))
        .map(|digit| digit as u8)
        .filter(|tone| (1..=5).contains(tone))
}

fn with_tone(key: &str, tone: u8) -> String {
    format!("{}{}", base_of(key), tone)
}

fn normalize(syllable: &str) -> String {
    syllable.to_lowercase().replace("u:", "ü").replace('v', "ü")
}

fn split_tone(syllable: &str) -> (&str, Option<u8>) {
    match syllable.chars().last().and_then(|c| c.to_digit(10)) {
        Some(digit) if (1..=5).contains(&digit) => {
            let end = syllable.len() - 1;
            (&syllable[..end], Some(digit as u8))
        },
        _ => (syllable, None),
    }
}

fn mark_position(chars: &[char]) -> Option<usize> {
    let find = |needle: char| chars.iter().position(|c| *c == needle);

    // a > o > e, then iu -> u, ui -> i, then the remaining vowel.
    if let Some(index) = find('a') {
        return Some(index);
    }
    if let Some(index) = find('o') {
        return Some(index);
    }
    if let Some(index) = find('e') {
        return Some(index);
    }
    if let Some(index) = find('i') {
        if chars.get(index + 1) == Some(&'u') {
            return Some(index + 1);
        }
    }
    if let Some(index) = find('u') {
        if chars.get(index + 1) == Some(&'i') {
            return Some(index + 1);
        }
    }
    if let Some(index) = find('i') {
        return Some(index);
    }
    if let Some(index) = find('ü') {
        return Some(index);
    }
    find('u')
}

fn add_tone(vowel: char, tone: u8) -> char {
    match (vowel, tone) {
        ('a', 1) => 'ā',
        ('a', 2) => 'á',
        ('a', 3) => 'ǎ',
        ('a', 4) => 'à',
        ('o', 1) => 'ō',
        ('o', 2) => 'ó',
        ('o', 3) => 'ǒ',
        ('o', 4) => 'ò',
        ('e', 1) => 'ē',
        ('e', 2) => 'é',
        ('e', 3) => 'ě',
        ('e', 4) => 'è',
        ('i', 1) => 'ī',
        ('i', 2) => 'í',
        ('i', 3) => 'ǐ',
        ('i', 4) => 'ì',
        ('u', 1) => 'ū',
        ('u', 2) => 'ú',
        ('u', 3) => 'ǔ',
        ('u', 4) => 'ù',
        ('ü', 1) => 'ǖ',
        ('ü', 2) => 'ǘ',
        ('ü', 3) => 'ǚ',
        ('ü', 4) => 'ǜ',
        _ => vowel,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(raw: &[&str]) -> Vec<String> {
        raw.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn marks_basic_syllables() {
        assert_eq!(marks("ni3 hao3"), "nǐ hǎo");
        assert_eq!(marks("xue2"), "xué");
        assert_eq!(marks("shui3"), "shuǐ");
        assert_eq!(marks("liu4"), "liù");
        assert_eq!(marks("gui4"), "guì");
        assert_eq!(marks("de5"), "de");
    }

    #[test]
    fn marks_umlaut() {
        assert_eq!(marks("lv4"), "lǜ");
        assert_eq!(marks("nu:3"), "nǚ");
    }

    #[test]
    fn marks_erhua() {
        assert_eq!(marks("na3 r5"), "nǎr");
        assert_eq!(marks("yi1 hui4 r5"), "yī huìr");
    }

    #[test]
    fn sandhi_third_tone_pair() {
        assert_eq!(sandhi(&keys(&["ni3", "hao3"])), keys(&["ni2", "hao3"]));
    }

    #[test]
    fn sandhi_bu() {
        assert_eq!(sandhi(&keys(&["bu4", "shi4"])), keys(&["bu2", "shi4"]));
        assert_eq!(sandhi(&keys(&["bu4", "hao3"])), keys(&["bu4", "hao3"]));
    }

    #[test]
    fn sandhi_yi() {
        assert_eq!(sandhi(&keys(&["yi1", "ge4"])), keys(&["yi2", "ge4"]));
        assert_eq!(sandhi(&keys(&["yi1", "tian1"])), keys(&["yi4", "tian1"]));
        assert_eq!(sandhi(&keys(&["yi1"])), keys(&["yi1"]));
    }

    #[test]
    fn base_and_tone() {
        assert_eq!(base_of("zhong1"), "zhong");
        assert_eq!(tone_of("zhong1"), Some(1));
        assert_eq!(tone_of("de5"), Some(5));
        assert_eq!(tone_of("ma"), None);
    }
}
