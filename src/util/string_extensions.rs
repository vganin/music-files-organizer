use strsim::normalized_damerau_levenshtein;
use unidecode::unidecode;

pub trait StringExtensions {
    fn simplify(&self) -> String;
    fn is_similar(&self, other: &str) -> bool;
}

impl StringExtensions for str {
    fn simplify(&self) -> String {
        unidecode(self).to_lowercase()
    }

    fn is_similar(&self, other: &str) -> bool {
        const SIMILAR_SCORE: f64 = 0.9f64;
        let self_simplified = self.simplify();
        let other_simplified = other.simplify();
        let score = normalized_damerau_levenshtein(&self_simplified, &other_simplified);
        score >= SIMILAR_SCORE
    }
}
