//! EmbeddingGemma 2 prompt templates (spec sec. 7, model card). Queries and documents use different prompts;
//! a vector store records [`PROMPT_VERSION`] so a template change rebuilds every vector (spec sec. 9).

/// Bump when either template changes.
pub const PROMPT_VERSION: u32 = 1;

/// Title placeholder for chunks without a title (model card wording).
pub const NO_TITLE: &str = "none";

/// `task: search result | query: {q}`.
#[must_use]
pub fn query_prompt(query: &str) -> String {
    format!("task: search result | query: {}", query.trim())
}

/// `title: {title|none} | text: {chunk}`. A missing or blank title becomes `none`.
#[must_use]
pub fn document_prompt(title: Option<&str>, text: &str) -> String {
    let title = title.map(str::trim).filter(|t| !t.is_empty());
    format!(
        "title: {} | text: {}",
        title.unwrap_or(NO_TITLE),
        text.trim()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_template_is_exact() {
        assert_eq!(
            query_prompt("how does grappling work"),
            "task: search result | query: how does grappling work"
        );
    }

    #[test]
    fn document_template_is_exact() {
        assert_eq!(
            document_prompt(Some("Grappled"), "A grappled creature's speed is 0."),
            "title: Grappled | text: A grappled creature's speed is 0."
        );
        assert_eq!(document_prompt(None, "text"), "title: none | text: text");
        assert_eq!(
            document_prompt(Some("  "), "text"),
            "title: none | text: text"
        );
    }
}
