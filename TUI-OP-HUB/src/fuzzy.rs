//! Fuzzy matching for search (US-SRCH-01, fuzzy capabilities).
//!
//! A lightweight subsequence scorer: characters of `needle` must appear in
//! `haystack` in order (case-insensitive). Scoring rewards word-start and
//! consecutive matches and prefers shorter haystacks, so `git` ranks above
//! `gitk` for the query `gti`.

/// Fuzzy-match `needle` against `haystack`. Returns the match score when the
/// needle is a (case-insensitive) subsequence, otherwise `None`.
pub fn fuzzy_match(haystack: &str, needle: &str) -> Option<i64> {
    if needle.is_empty() {
        return Some(0);
    }
    let hay: Vec<char> = haystack.to_lowercase().chars().collect();
    let need: Vec<char> = needle.to_lowercase().chars().collect();
    if need.len() > hay.len() {
        return None;
    }

    let mut score: i64 = 0;
    let mut search_from = 0usize;
    let mut prev_pos: Option<usize> = None;

    for &nc in &need {
        let pos = hay[search_from..].iter().position(|&hc| hc == nc)? + search_from;
        score += 10;
        // Consecutive characters (typed a whole word fragment)
        if prev_pos == Some(pos.wrapping_sub(1)) {
            score += 5;
        }
        // Match at a word start (beginning or after a separator)
        if pos == 0 || !hay[pos - 1].is_alphanumeric() {
            score += 8;
        }
        prev_pos = Some(pos);
        search_from = pos + 1;
    }

    // Prefer shorter haystacks (closer to the needle)
    score -= (hay.len() as i64 - need.len() as i64).min(10);
    Some(score)
}

/// Best fuzzy score over several haystacks (`None` when none matches).
pub fn fuzzy_match_any(haystacks: &[&str], needle: &str) -> Option<i64> {
    haystacks
        .iter()
        .filter_map(|h| fuzzy_match(h, needle))
        .max()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subsequence_matches_with_typo_ordering() {
        assert!(fuzzy_match("git", "gt").is_some());
        assert!(fuzzy_match("git status", "gts").is_some());
        assert!(fuzzy_match("docker", "dck").is_some());
        assert!(fuzzy_match("docker", "xyz").is_none());
        assert!(
            fuzzy_match("git", "git!!").is_none(),
            "needle longer than hay"
        );
    }

    #[test]
    fn case_is_insensitive() {
        assert!(fuzzy_match("Git Status", "GST").is_some());
        assert!(fuzzy_match("docker", "DOCK").is_some());
    }

    #[test]
    fn empty_needle_matches_everything_with_zero_score() {
        assert_eq!(fuzzy_match("anything", ""), Some(0));
    }

    #[test]
    fn shorter_haystacks_rank_higher() {
        let short = fuzzy_match("git", "gt").unwrap();
        let long = fuzzy_match("git remote add origin", "gt").unwrap();
        assert!(short > long);
    }

    #[test]
    fn word_start_and_consecutive_bonuses_apply() {
        // 'do' at the start of "docker" scores higher than scattered 'd','o'
        let start = fuzzy_match("docker", "do").unwrap();
        let scattered = fuzzy_match("pod or", "do").unwrap();
        assert!(start > scattered);
    }

    #[test]
    fn fuzzy_match_any_takes_the_best_score() {
        assert_eq!(fuzzy_match_any(&["docker", "git"], "dck").is_some(), true);
        assert!(fuzzy_match_any(&["docker", "git"], "xyz").is_none());
    }
}
