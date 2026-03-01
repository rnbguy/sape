use rand::Rng;

const WORDS: [&str; 256] = [
    "acorn", "adapt", "adobe", "agent", "agile", "amber", "anchor", "apple", "april", "arrow",
    "atlas", "audio", "baker", "basic", "beach", "beacon", "berry", "bison", "blade", "blaze",
    "block", "bloom", "board", "brave", "brick", "brook", "brush", "cabin", "cable", "camel",
    "candy", "cannon", "cargo", "cedar", "charm", "chess", "chili", "cider", "civic", "clean",
    "clerk", "cloud", "coast", "color", "comet", "coral", "crown", "curve", "daily", "daisy",
    "delta", "diner", "dolphin", "donut", "drift", "eager", "eagle", "earth", "elder", "ember",
    "engine", "entry", "equal", "falcon", "fancy", "ferry", "fiber", "field", "final", "flame",
    "flash", "flora", "focus", "forest", "fossil", "frame", "frost", "galaxy", "gamma", "garden",
    "gentle", "glade", "glider", "globe", "golden", "grain", "grape", "grass", "groove", "harbor",
    "hazel", "helium", "hollow", "honey", "hotel", "humble", "iceberg", "iconic", "igloo",
    "island", "ivory", "jacket", "jelly", "jewel", "jolly", "jungle", "kernel", "kiwi", "ladder",
    "laser", "legend", "lemon", "level", "linen", "lively", "lizard", "lobster", "locust", "lunar",
    "magnet", "mango", "maple", "market", "meadow", "melon", "merit", "meteor", "mint", "mosaic",
    "motion", "mount", "museum", "nacho", "native", "nectar", "nickel", "noble", "noodle",
    "normal", "oasis", "ocean", "olive", "onyx", "opal", "orbit", "orchid", "origin", "otter",
    "paddle", "panda", "panel", "paper", "parade", "parcel", "pearl", "pepper", "petal", "phoenix",
    "piano", "pilot", "pixel", "planet", "plaza", "pocket", "poem", "polar", "ponder", "prairie",
    "prism", "pulse", "quartz", "quill", "rabbit", "radar", "raven", "reef", "relay", "ribbon",
    "ripple", "river", "rocket", "royal", "saddle", "sailor", "salmon", "satin", "scenic",
    "shadow", "shiny", "signal", "silver", "simple", "sketch", "solar", "sonic", "spark", "spruce",
    "stable", "star", "stone", "storm", "sunset", "symbol", "tactic", "talon", "tango", "temple",
    "thunder", "timber", "titan", "token", "topaz", "trail", "tulip", "turbo", "ultra", "umbra",
    "unison", "velvet", "vertex", "vessel", "vivid", "voyage", "walnut", "wave", "whale",
    "whisper", "window", "winter", "wizard", "woven", "xenon", "yarrow", "yellow", "yonder",
    "almond", "anthem", "badge", "banjo", "breeze", "button", "canary", "circle", "copper",
    "cosmos", "dynamo", "elmwood", "feather", "gadget", "hammer", "impact", "jasper", "lantern",
    "marble", "nimbus", "zephyr",
];

#[must_use]
pub fn generate_code() -> String {
    let mut rng = rand::thread_rng();
    let number = rng.gen_range(0..100);
    let word1 = WORDS[rng.gen_range(0..WORDS.len())];
    let word2 = WORDS[rng.gen_range(0..WORDS.len())];
    format!("{number}-{word1}-{word2}")
}

#[must_use]
pub fn is_pairing_code(s: &str) -> bool {
    let mut parts = s.split('-');
    let Some(number_part) = parts.next() else {
        return false;
    };
    let Some(word1) = parts.next() else {
        return false;
    };
    let Some(word2) = parts.next() else {
        return false;
    };

    if parts.next().is_some() {
        return false;
    }

    number_part.parse::<u8>().is_ok_and(|n| n <= 99)
        && WORDS.contains(&word1)
        && WORDS.contains(&word2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_code_format() {
        let code = generate_code();
        let parts: Vec<&str> = code.split('-').collect();
        assert_eq!(parts.len(), 3, "code must have 3 parts: {code}");
        let num: u8 = parts[0].parse().expect("first part must be a number");
        assert!(num <= 99, "number must be 0-99");
        assert!(WORDS.contains(&parts[1]), "word1 must be from wordlist");
        assert!(WORDS.contains(&parts[2]), "word2 must be from wordlist");
    }

    #[test]
    fn generate_code_uniqueness() {
        let codes: std::collections::HashSet<String> = (0..100).map(|_| generate_code()).collect();
        assert!(codes.len() > 1, "codes should be unique");
    }

    #[test]
    fn is_pairing_code_valid() {
        assert!(is_pairing_code("42-river-ocean"));
        assert!(is_pairing_code("0-acorn-zephyr"));
        assert!(is_pairing_code("99-ember-frost"));
    }

    #[test]
    fn is_pairing_code_invalid() {
        assert!(!is_pairing_code(""));
        assert!(!is_pairing_code("notacode"));
        assert!(!is_pairing_code("42-river"));
        assert!(!is_pairing_code("42-river-ocean-extra"));
        assert!(!is_pairing_code("abc-river-ocean"));
        assert!(!is_pairing_code("100-river-ocean"));
        assert!(!is_pairing_code("42-notaword-ocean"));
        assert!(!is_pairing_code("42-river-notaword"));
        assert!(!is_pairing_code("/ip4/1.2.3.4/tcp/4001"));
        assert!(!is_pairing_code("/mdns/QmSomePeerId"));
    }

    #[test]
    fn wordlist_no_duplicates() {
        let mut seen = std::collections::HashSet::new();
        for word in &WORDS {
            assert!(seen.insert(word), "duplicate word: {word}");
        }
    }

    #[test]
    fn wordlist_all_lowercase_ascii() {
        for word in &WORDS {
            assert!(
                word.chars().all(|c| c.is_ascii_lowercase()),
                "word must be lowercase ASCII: {word}"
            );
        }
    }

    #[test]
    fn wordlist_length_bounds() {
        for word in &WORDS {
            assert!(word.len() >= 3, "word too short: {word}");
            assert!(word.len() <= 8, "word too long: {word}");
        }
    }
}
