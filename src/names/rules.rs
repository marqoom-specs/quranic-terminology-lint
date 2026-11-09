//! The rules of the naming standard that the names check enforces.

use std::fmt;

/// A rule of the naming standard, in the order of its number.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Rule {
    OneName,
    Characters,
    TaMarbutah,
    TaMarbutahInIdafah,
    LongVowels,
    Nisbah,
    FinalHamzah,
    SunLetters,
    LeadingArticle,
    ArticleOnAdjective,
    ArticleInIdafah,
    Stability,
    Deprecated,
    Plural,
    TypeSuffix,
    NumberingNames,
    PersonalNames,
    SurahNames,
    ReferenceNumbers,
    Position,
    AudioTimings,
    ClearNames,
}

use Rule::*;

const ALL: [Rule; 22] = [
    OneName, Characters, TaMarbutah, TaMarbutahInIdafah, LongVowels, Nisbah, FinalHamzah, SunLetters,
    LeadingArticle, ArticleOnAdjective, ArticleInIdafah, Stability, Deprecated, Plural, TypeSuffix,
    NumberingNames, PersonalNames, SurahNames, ReferenceNumbers, Position, AudioTimings, ClearNames,
];

impl Rule {
    /// The rule's number in the standard, or its name when it has none.
    pub fn code(self) -> &'static str {
        match self {
            OneName => "001",
            Characters => "009",
            TaMarbutah => "019",
            TaMarbutahInIdafah => "020",
            LongVowels => "021",
            Nisbah => "022",
            FinalHamzah => "026",
            SunLetters => "030",
            LeadingArticle => "031",
            ArticleOnAdjective => "032",
            ArticleInIdafah => "033",
            Stability => "037",
            Deprecated => "046",
            Plural => "049",
            TypeSuffix => "050",
            NumberingNames => "055",
            PersonalNames => "056",
            SurahNames => "057",
            ReferenceNumbers => "068",
            Position => "069",
            AudioTimings => "073",
            ClearNames => "clear-names",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            OneName => "One code name per concept",
            Characters => "Characters in code spelling",
            TaMarbutah => "Ta marbutah outside idafah",
            TaMarbutahInIdafah => "Ta marbutah in idafah",
            LongVowels => "Long vowels",
            Nisbah => "The nisbah ending",
            FinalHamzah => "Final hamzah and ayn after a consonant with sukun",
            SunLetters => "The definite article and sun letters",
            LeadingArticle => "The definite article at the beginning",
            ArticleOnAdjective => "The definite article on an adjective",
            ArticleInIdafah => "The definite article in idafah",
            Stability => "Code-name stability",
            Deprecated => "Deprecated names",
            Plural => "Forming the code plural",
            TypeSuffix => "Classification names and the _type suffix",
            NumberingNames => "Ayah numbering system names",
            PersonalNames => "Personal names",
            SurahNames => "Deriving surah names",
            ReferenceNumbers => "Reference numbers",
            Position => "Position in a sequence",
            AudioTimings => "Naming audio timings",
            ClearNames => "Clear names",
        }
    }

    /// Rules about code names alone: prose writes "al-Fatihah", "Qur'an" and
    /// scholarly transliteration (`riwāyāt`) normally, and shows display names.
    fn code_only(self) -> bool {
        matches!(self, Characters | FinalHamzah | SunLetters | LeadingArticle | ArticleOnAdjective | ArticleInIdafah | Stability)
    }
}

impl fmt::Display for Rule {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ClearNames => f.write_str(self.title()),
            _ => write!(f, "rule {}: {}", self.code(), self.title()),
        }
    }
}

/// A set of rules, iterated in the order of their numbers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Rules(u32);

impl Rules {
    pub const NONE: Rules = Rules(0);

    pub fn with(self, rule: Rule) -> Rules {
        Rules(self.0 | 1 << rule as u32)
    }

    pub fn union(self, other: Rules) -> Rules {
        Rules(self.0 | other.0)
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Exactly this one rule.
    pub fn is(self, rule: Rule) -> bool {
        self == Rules::from(rule)
    }

    pub fn contains(self, rule: Rule) -> bool {
        self.0 & 1 << rule as u32 != 0
    }

    pub fn iter(self) -> impl Iterator<Item = Rule> {
        ALL.into_iter().filter(move |r| self.contains(*r))
    }

    pub fn code_only(self) -> bool {
        self.iter().all(Rule::code_only)
    }

    /// "rule 019: Ta marbutah outside idafah; rule 068: Reference numbers"
    pub fn message(self) -> String {
        self.iter().map(|r| r.to_string()).collect::<Vec<_>>().join("; ")
    }
}

impl From<Rule> for Rules {
    fn from(rule: Rule) -> Rules {
        Rules::NONE.with(rule)
    }
}

impl FromIterator<Rule> for Rules {
    fn from_iter<I: IntoIterator<Item = Rule>>(iter: I) -> Rules {
        iter.into_iter().fold(Rules::NONE, Rules::with)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_iterate_in_number_order() {
        let rules: Rules = [ClearNames, TaMarbutah, OneName].into_iter().collect();
        assert_eq!(rules.iter().map(Rule::code).collect::<Vec<_>>(), ["001", "019", "clear-names"]);
        assert_eq!(Rules::from(TaMarbutah).message(), "rule 019: Ta marbutah outside idafah");
    }
}
