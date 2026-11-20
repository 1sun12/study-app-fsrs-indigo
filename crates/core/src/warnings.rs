//! Authoring guard rails (SPEC §9.2): warn, never block.

use crate::domain::{Card, Response};

pub const TYPED_REFERENCE_MAX_LINES: usize = 15;
pub const EST_MINUTES_WARN: u32 = 45;

pub fn card_warnings(card: &Card) -> Vec<String> {
    let mut out = Vec::new();
    for (i, step) in card.steps.iter().enumerate() {
        let n = i + 1;
        if let Some(text) = &step.solution_text
            && text.lines().count() > TYPED_REFERENCE_MAX_LINES
        {
            out.push(format!(
                "Step {n}: the typed reference answer is longer than {TYPED_REFERENCE_MAX_LINES} lines; consider splitting the card."
            ));
        }
        if step.response == Response::Recall
            && step
                .solution_text
                .as_deref()
                .unwrap_or("")
                .trim()
                .is_empty()
            && step.solution_md.trim().is_empty()
            && !step
                .media
                .iter()
                .any(|m| m.role == crate::domain::MediaRole::Solution)
        {
            out.push(format!(
                "Step {n}: there is no answer (text or media) to compare yourself against."
            ));
        }
        if card.steps.len() > 1
            && step.criteria.is_empty()
            && step.solution_md.trim().is_empty()
            && step
                .solution_text
                .as_deref()
                .unwrap_or("")
                .trim()
                .is_empty()
        {
            out.push(format!(
                "Step {n}: in a multi-step card every step needs criteria or a solution."
            ));
        }
    }
    if card.est_minutes.is_some_and(|m| m > EST_MINUTES_WARN) {
        out.push(format!(
            "Estimated time is over {EST_MINUTES_WARN} minutes; long cards rarely get reviewed."
        ));
    }
    out
}
