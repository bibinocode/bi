/// 不区分大小写的子序列匹配。分数越高越优先，连续字符和单词开头加分。
pub fn fuzzy_score(query: &str, candidate: &str) -> Option<i64> {
    let query = query.to_lowercase();
    let candidate = candidate.to_lowercase();
    let mut wanted = query.chars().peekable();
    let mut score = 0;
    let mut previous_match = false;
    let mut boundary = true;
    for (index, ch) in candidate.chars().enumerate() {
        if wanted.peek() == Some(&ch) {
            wanted.next();
            score += 10 + if previous_match { 15 } else { 0 } + if boundary { 10 } else { 0 };
            score -= index as i64;
            previous_match = true;
        } else {
            previous_match = false;
        }
        boundary = ch.is_whitespace() || matches!(ch, '/' | '\\' | '-' | '_' | '.');
    }
    wanted.next().is_none().then_some(score)
}
