//! Generated device passwords: `word-word-NN` ("maple-otter-42"), easy to
//! say across a campfire and to type on a phone.
//!
//! Two words from a list of 128 and a number from 10 to 99: about 20
//! bits. That is a play password for someone nearby, behind the board's
//! login backoff and a 60 000-round key derivation — not a vault. The
//! randomness is the caller's (the page passes the browser's crypto), so
//! the words are testable.

/// Short, concrete, unambiguous words: no homophones, nothing rude, none
/// that autocorrect likes to change.
const WORDS: [&str; 128] = [
    "amber", "anchor", "apple", "arrow", "aspen", "badger", "bamboo", "banjo", "basil", "beacon",
    "birch", "bison", "blossom", "breeze", "brook", "cactus", "camel", "candle", "canyon", "cedar",
    "cherry", "cinder", "clover", "cobalt", "comet", "coral", "cosmos", "cricket", "crystal",
    "daisy", "delta", "desert", "dingo", "dragon", "drum", "dune", "eagle", "ember", "falcon",
    "fern", "fiddle", "firefly", "flint", "forest", "fox", "galaxy", "garnet", "gecko", "ginger",
    "glacier", "glow", "granite", "harbor", "hazel", "heron", "honey", "iris", "island", "ivy",
    "jade", "jasper", "jungle", "kayak", "kelp", "kettle", "kiwi", "lagoon", "lantern", "lemon",
    "lily", "lotus", "lunar", "lynx", "magnet", "mango", "maple", "marble", "meadow", "mesa",
    "mint", "moose", "moss", "nectar", "nova", "oak", "ocean", "olive", "onyx", "orbit", "orchid",
    "otter", "owl", "panda", "pebble", "pepper", "pine", "planet", "plum", "pony", "prism",
    "quartz", "rabbit", "raven", "reef", "river", "robin", "rocket", "saffron", "sage", "salmon",
    "sierra", "sparrow", "spruce", "summit", "sunset", "thistle", "tiger", "topaz", "tulip",
    "tundra", "velvet", "violet", "walnut", "willow", "yak", "zebra", "zephyr", "zinnia",
];

/// A fresh password from four random bytes.
pub(crate) fn share_words(random: [u8; 4]) -> String {
    let first = WORDS[usize::from(random[0]) % WORDS.len()];
    let mut second = WORDS[usize::from(random[1]) % WORDS.len()];
    if second == first {
        second = WORDS[(usize::from(random[1]) + 1) % WORDS.len()];
    }
    let number = 10 + u16::from_le_bytes([random[2], random[3]]) % 90;
    format!("{first}-{second}-{number}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_reads_word_word_number() {
        let words = share_words([0, 1, 2, 0]);
        let parts: Vec<&str> = words.split('-').collect();
        assert_eq!(parts.len(), 3, "{words}");
        assert!(WORDS.contains(&parts[0]) && WORDS.contains(&parts[1]));
        let number: u32 = parts[2].parse().unwrap();
        assert!((10..=99).contains(&number));
    }

    #[test]
    fn the_two_words_differ() {
        let words = share_words([5, 5, 0, 0]);
        let parts: Vec<&str> = words.split('-').collect();
        assert_ne!(parts[0], parts[1]);
    }

    #[test]
    fn every_word_is_lowercase_and_unique() {
        let mut sorted = WORDS.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), WORDS.len());
        assert!(
            WORDS
                .iter()
                .all(|word| word.chars().all(|c| c.is_ascii_lowercase()))
        );
    }
}
