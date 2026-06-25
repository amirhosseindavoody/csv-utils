/// Subsequence fuzzy score; higher is a better match. Empty query matches everything.
pub fn fuzzy_score(query: &str, target: &str) -> Option<u32> {
    if query.is_empty() {
        return Some(0);
    }
    let q: Vec<char> = query.to_ascii_lowercase().chars().collect();
    let t: Vec<char> = target.to_ascii_lowercase().chars().collect();
    let mut qi = 0;
    let mut score = 0u32;
    let mut prev_match = false;
    let mut run = 0u32;
    for (ti, &tc) in t.iter().enumerate() {
        if qi < q.len() && tc == q[qi] {
            qi += 1;
            if prev_match {
                run += 1;
                score += run;
            } else if ti == 0 {
                score += 10;
            } else {
                let prev = t[ti - 1];
                if !prev.is_ascii_alphanumeric() {
                    score += 10;
                }
            }
            score += 1;
            prev_match = true;
        } else {
            prev_match = false;
            run = 0;
        }
    }
    if qi == q.len() {
        Some(score)
    } else {
        None
    }
}

pub fn rank_by_fuzzy<'a>(
    query: &str,
    items: impl Iterator<Item = (usize, &'a str)>,
) -> Vec<usize> {
    let mut scored: Vec<(usize, u32)> = items
        .filter_map(|(idx, name)| fuzzy_score(query, name).map(|score| (idx, score)))
        .collect();
    scored.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    scored.into_iter().map(|(idx, _)| idx).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_query_matches_all() {
        assert_eq!(fuzzy_score("", "anything"), Some(0));
    }

    #[test]
    fn subsequence_match_scores() {
        assert!(fuzzy_score("abc", "a_big_cat").unwrap() > fuzzy_score("abc", "axbxc").unwrap());
        assert!(fuzzy_score("xyz", "alpha").is_none());
    }

    #[test]
    fn ranks_best_match_first() {
        let items = vec![(0, "id"), (1, "city"), (2, "city_code")];
        let ranked = rank_by_fuzzy("city", items.into_iter());
        assert_eq!(ranked[0], 1);
        assert!(ranked.contains(&2));
    }
}
