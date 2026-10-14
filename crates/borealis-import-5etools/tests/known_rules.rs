//! Loud content (engine-test-audit-1 WP1): the importer emits only custom rule kinds and fields
//! the engine reads (`borealis_content::rule_kinds::KNOWN_KINDS`), and says so in its report.

mod support;

use borealis_content::rule_kinds::check_rules;
use support::*;

#[test]
fn srd_fixture_output_has_no_rules_the_engine_ignores() {
    let result = import_fixtures();
    let mut problems = Vec::new();
    let mut custom = 0;
    for pack in &result.packs {
        for el in pack.elements() {
            custom += el
                .rules
                .iter()
                .filter(|r| matches!(r, borealis_content::Rule::Custom(_)))
                .count();
            for p in check_rules(&el.rules) {
                problems.push(format!("{} ({}): {p}", el.id, el.ty));
            }
        }
    }
    assert!(custom > 0, "the fixtures produced no custom rules to check");
    assert!(problems.is_empty(), "{}", problems.join("\n"));
    assert!(
        result.report.errors.is_empty(),
        "report errors: {:?}",
        result.report.errors
    );
    // the gates the converter applied all landed on rules that read them
    let lost: Vec<_> = result
        .report
        .partial
        .iter()
        .filter(|n| n.reason.contains("the engine reads"))
        .collect();
    assert!(lost.is_empty(), "{lost:?}");
}
