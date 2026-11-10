//! Small, relevant, non-duplicated context; chat output is not a source of facts.
use fabric_core::ledger::Hit;
pub const MAX_ITEMS: usize = 2;
pub const MAX_CHARS: usize = 800;
pub const MIN_SCORE: f32 = 0.50;

pub fn select(mut hits: Vec<Hit>) -> Vec<Hit> {
    hits.sort_by(|a,b| b.score.total_cmp(&a.score));
    let mut selected = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut remaining = MAX_CHARS;
    let mut best: Option<f32> = None;
    for mut hit in hits {
        let chat = hit.memory.starts_with("mac-chat-") || hit.memory.starts_with("chat-");
        let text=hit.text.trim();
        let lower=text.to_lowercase();
        let question=chat && (text.ends_with('?') || ["is ","are ","was ","were ","do ","did ","does ","would ","should ","what ","who ","why ","when ","where ","how ","can you ","could you ","check ","fetch ","search ","tell me "].iter().any(|s|lower.starts_with(s)));
        if !hit.score.is_finite() || hit.score<MIN_SCORE || text.is_empty()
            || (chat && hit.memory.ends_with("-assistant")) || question {continue;}
        if best.is_some_and(|top| top-hit.score>0.08) {continue;}
        let key=lower.split_whitespace().collect::<Vec<_>>().join(" ");
        if !seen.insert(key) {continue;}
        best.get_or_insert(hit.score);
        hit.text=text.chars().take(400.min(remaining)).collect();
        remaining-=hit.text.chars().count();
        selected.push(hit);
        if selected.len()==MAX_ITEMS || remaining==0 {break;}
    }
    selected
}

#[cfg(test)]
mod tests {
    use super::*;
    fn hit(id:&str,text:&str,score:f32)->Hit { Hit{memory:id.into(),text:text.into(),score,origin:"fixture".into(),translated:false,access:Default::default()} }
    #[test]
    fn irrelevant_and_generated_chat_are_excluded() {
        assert!(select(vec![hit("note-low","Unrelated fact",0.49),hit("mac-chat-x-assistant","I cannot browse",0.99),hit("chat-x-user","What is my theme?",0.99)]).is_empty());
    }
    #[test]
    fn recall_has_a_shared_budget_and_deduplicates() {
        let long="x".repeat(1000);
        let selected=select(vec![hit("a",&long,0.9),hit("b",&long,0.89),hit("c",&"y".repeat(1000),0.88),hit("d","Third fact",0.87)]);
        assert_eq!(selected.len(),2);
        assert_eq!(selected.iter().map(|h|h.text.chars().count()).sum::<usize>(),800);
    }
    #[test]
    fn weak_secondary_matches_do_not_pad_context() {
        assert_eq!(select(vec![hit("a","My theme is Mint",0.85),hit("b","Different topic",0.60)]).len(),1);
    }
}
