use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use strsim::normalized_damerau_levenshtein;
use unidecode::unidecode;

pub trait StringExtensions {
    fn simplify(&self) -> String;
    fn is_similar(&self, other: &str) -> bool;
    fn remove_special_chars(&self) -> String;
    fn remove_excessive_whitespaces(&self) -> String;
}

impl StringExtensions for str {
    fn simplify(&self) -> String {
        unidecode(self)
            .to_lowercase()
            .remove_special_chars()
            .remove_excessive_whitespaces()
    }

    fn is_similar(&self, other: &str) -> bool {
        const SIMILAR_SCORE: f64 = 0.9f64;
        let self_simplified = self.simplify();
        let other_simplified = other.simplify();
        let strings_are_similar = || {
            normalized_damerau_levenshtein(&self_simplified, &other_simplified) >= SIMILAR_SCORE
        };
        let strings_are_prefixes_of_each_other = || {
            let matcher = SkimMatcherV2::default();
            matcher.fuzzy_match(&self_simplified, &other_simplified).is_some()
                || matcher.fuzzy_match(&other_simplified, &self_simplified).is_some()
        };
        strings_are_similar() || strings_are_prefixes_of_each_other()
    }

    fn remove_special_chars(&self) -> String {
        self.chars()
            .filter(|c| c.is_ascii_alphanumeric() || c.is_ascii_whitespace())
            .collect::<String>()
    }

    fn remove_excessive_whitespaces(&self) -> String {
        self.replace("  ", " ")
    }
}
