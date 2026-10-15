//! Page furniture as the passages of a source are built from it: [`furniture`] without the
//! bare numbers that are content. A page number is printed once a page, so two bare-number
//! lines a few lines apart are a table column flattened one value per line ("2", "4", "6"),
//! never page marks.

use super::super::furniture::{furniture, Furniture};

/// Fewest lines between two page numbers (one per page).
const MIN_PAGE_GAP: usize = 10;
/// Most digits of a page number.
const MAX_PAGE_DIGITS: usize = 4;

/// The page furniture of a source, bare numbers that are content left in.
#[derive(Debug, Default, Clone)]
pub(crate) struct PageFurniture {
    all: Furniture,
    content: Vec<usize>,
}

impl PageFurniture {
    /// The line at `i` (0-based) is page furniture.
    pub(crate) fn has(&self, i: usize) -> bool {
        self.all.has(i) && !self.content.contains(&i)
    }
}

fn bare_number(line: &str) -> bool {
    let t = line.trim();
    (1..=MAX_PAGE_DIGITS).contains(&t.len()) && t.chars().all(|c| c.is_ascii_digit())
}

/// The page furniture of `source` (see the module doc).
pub(crate) fn page_furniture(source: &str) -> PageFurniture {
    let all = furniture(source);
    let numbers: Vec<usize> = source
        .lines()
        .enumerate()
        .filter(|(i, l)| all.has(*i) && bare_number(l))
        .map(|(i, _)| i)
        .collect();
    let content = numbers
        .iter()
        .enumerate()
        .filter(|(k, &i)| {
            let near = |j: Option<&usize>| j.is_some_and(|&j: &usize| j.abs_diff(i) < MIN_PAGE_GAP);
            near(k.checked_sub(1).and_then(|p| numbers.get(p))) || near(numbers.get(k + 1))
        })
        .map(|(_, &i)| i)
        .collect();
    PageFurniture { all, content }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_column_of_bare_numbers_is_content_and_spaced_page_numbers_are_not() {
        let mut src = String::new();
        for page in [3, 4, 5] {
            src.push_str(&"A line of the page's running prose.\n\n".repeat(6));
            src.push_str(&format!("{page}\n\n"));
        }
        src.push_str(&"More running prose on the next page.\n\n".repeat(4));
        src.push_str("#### Hours by Effort\n\nHours\n\n2\n\n4\n\n8\n\n12\n\nRoll\n\nOnce\n");
        let marks = page_furniture(&src);
        let at = |t: &str| src.lines().position(|l| l == t).unwrap();
        for page in ["3", "4", "5"] {
            assert!(marks.has(at(page)), "{page}");
        }
        let column = src
            .lines()
            .collect::<Vec<_>>()
            .iter()
            .rposition(|l| *l == "4")
            .unwrap();
        assert!(
            furniture(&src).has(column),
            "the raw filter takes it for a page"
        );
        assert!(!marks.has(column));
        for value in ["2", "8", "12"] {
            assert!(!marks.has(at(value)), "{value}");
        }
    }
}
