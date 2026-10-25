//! What a failed index download tells the user: one short sentence per cause for the Indexes
//! list and the status bar (no connection, an HTTP status, a damaged download), built from the
//! classified [`FetchError`] and never from the transport's text. The transport's error chain goes
//! to the log only ([`crate::content_index::send_error`]).

use crate::content_fetch_retry::{
    FetchError, CODE_NETWORK, CODE_RATE_LIMITED, CODE_REFUSED, CODE_TIMEOUT, CODE_TOO_LARGE,
    CODE_TRUNCATED,
};

/// The top-level `.index` file of a content source.
pub const INDEX_FILE: &str = "the index file";
/// The detached signature served next to a Borealis index (`<index>.sig`).
pub const INDEX_SIGNATURE: &str = "the index signature";

/// No connection, a timeout or a download cut short.
pub const CHECK_CONNECTION: &str = "Check your connection and try again.";
const RATE_LIMITED: &str = "The server is limiting downloads. Try again later.";
const TOO_LARGE: &str = "The file is larger than Borealis accepts.";
const REDIRECTED: &str =
    "The server redirected to an address that is not allowed (only https:// addresses are).";
const OTHER: &str = "Try again later.";

/// "Could not download `what`." and why, in plain words.
#[must_use]
pub fn download_failed(what: &str, e: &FetchError) -> String {
    format!("Could not download {what}. {}", cause(e))
}

/// `what` arrived but cannot be read (cut off, damaged, or not an index at all).
#[must_use]
pub fn damaged(what: &str) -> String {
    format!("Could not read {what}. The download is damaged or is not an index file.")
}

fn cause(e: &FetchError) -> String {
    match (e.status(), e.code.as_str()) {
        (Some(429), _) | (_, CODE_RATE_LIMITED) => RATE_LIMITED.into(),
        (Some(status @ (404 | 410)), _) => {
            format!("The server has no file at this address (error {status}).")
        }
        (Some(status @ (401 | 403)), _) => {
            format!("The server refused access (error {status}).")
        }
        (Some(status @ (408 | 425 | 500..=599)), _) => {
            format!("The server is not available right now (error {status}). Try again later.")
        }
        (Some(status), _) => format!("The server answered with error {status}."),
        (None, CODE_NETWORK | CODE_TIMEOUT | CODE_TRUNCATED) => CHECK_CONNECTION.into(),
        (None, CODE_TOO_LARGE) => TOO_LARGE.into(),
        (None, CODE_REFUSED) => REDIRECTED.into(),
        _ => OTHER.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content_fetch_retry::CODE_OTHER;

    #[test]
    fn every_cause_has_a_short_sentence() {
        let cases = [
            (FetchError::network("dns error"), CHECK_CONNECTION),
            (FetchError::timeout("deadline"), CHECK_CONNECTION),
            (FetchError::truncated("eof"), CHECK_CONNECTION),
            (FetchError::rate_limited(None), RATE_LIMITED),
            (FetchError::too_large(), TOO_LARGE),
            (
                FetchError::new(CODE_REFUSED, "too many redirects"),
                REDIRECTED,
            ),
            (FetchError::new(CODE_OTHER, "a stub's own words"), OTHER),
        ];
        for (e, want) in cases {
            assert_eq!(
                download_failed(INDEX_FILE, &e),
                format!("Could not download the index file. {want}")
            );
        }
    }

    #[test]
    fn http_statuses_are_grouped_and_named() {
        let text = |s: u16| download_failed(INDEX_SIGNATURE, &FetchError::http(s, "", None));
        assert_eq!(
            text(410),
            "Could not download the index signature. The server has no file at this address (error 410)."
        );
        assert_eq!(
            text(401),
            "Could not download the index signature. The server refused access (error 401)."
        );
        assert!(text(500).ends_with("not available right now (error 500). Try again later."));
        assert!(text(408).ends_with("not available right now (error 408). Try again later."));
        assert!(text(429).ends_with(RATE_LIMITED));
        assert!(text(418).ends_with("The server answered with error 418."));
    }

    #[test]
    fn a_message_never_repeats_the_transport_text() {
        let e = FetchError::from_message(
            "could not connect: error sending request for url (https://x.invalid/a.index)",
        );
        let text = download_failed(INDEX_FILE, &e);
        assert_eq!(
            text,
            "Could not download the index file. Check your connection and try again."
        );
    }
}
