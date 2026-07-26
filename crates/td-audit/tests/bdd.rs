//! BDD acceptance tests for the audit checkers (cucumber / Gherkin).
//! Pure-engine scenarios — no server, no DB.

use cucumber::{given, then, when, World};
use td_audit::compose::audit_compose;
use td_audit::schema::{AuditReport, Grade, Severity};

#[derive(Debug, Default, World)]
struct AuditWorld {
    input: String,
    report: Option<AuditReport>,
}

impl AuditWorld {
    fn report(&self) -> &AuditReport {
        self.report.as_ref().expect("audit must run before assertions")
    }
}

fn severity_from(word: &str) -> Severity {
    match word {
        "critical" => Severity::Critical,
        "warning" => Severity::Warning,
        _ => Severity::Info,
    }
}

fn grade_from(letter: &str) -> Grade {
    match letter {
        "A" => Grade::A,
        "B" => Grade::B,
        "C" => Grade::C,
        "D" => Grade::D,
        _ => Grade::F,
    }
}

#[given(regex = r#"^the compose fixture "([^"]+)"$"#)]
async fn given_fixture(world: &mut AuditWorld, name: String) {
    let path = format!(
        "{}/tests/fixtures/compose/{}",
        env!("CARGO_MANIFEST_DIR"),
        name
    );
    world.input = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("fixture {path} unreadable: {e}"));
}

#[given("a compose document that is not valid YAML")]
async fn given_invalid(world: &mut AuditWorld) {
    world.input = ":\n  not: valid: yaml: [".to_string();
}

#[when("I audit the compose")]
async fn when_audit(world: &mut AuditWorld) {
    world.report = Some(audit_compose(&world.input));
}

#[then(regex = r#"^the grade is "([A-F])"$"#)]
async fn then_grade(world: &mut AuditWorld, grade: String) {
    assert_eq!(world.report().grade, grade_from(&grade));
}

#[then(regex = r#"^there is a "([a-z]+)" finding with id "([^"]+)"$"#)]
async fn then_finding(world: &mut AuditWorld, severity: String, id: String) {
    let sev = severity_from(&severity);
    let report = world.report();
    assert!(
        report.findings.iter().any(|f| f.id == id && f.severity == sev),
        "expected a {severity} finding '{id}', got: {:?}",
        report.findings
    );
}

#[then("there are no critical findings")]
async fn then_no_critical(world: &mut AuditWorld) {
    let report = world.report();
    assert!(
        report.findings.iter().all(|f| f.severity != Severity::Critical),
        "unexpected critical finding: {:?}",
        report.findings
    );
}

#[then(regex = r#"^the score is at least (\d+)$"#)]
async fn then_score(world: &mut AuditWorld, min: u32) {
    let score = world.report().score;
    assert!(score >= min, "score {score} < {min}");
}

#[tokio::main]
async fn main() {
    AuditWorld::cucumber()
        .fail_on_skipped()
        .run_and_exit("tests/features")
        .await;
}
