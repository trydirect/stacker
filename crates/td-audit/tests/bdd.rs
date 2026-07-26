//! BDD acceptance tests for the audit checkers (cucumber / Gherkin).
//! Pure-engine scenarios — no server, no DB.

use cucumber::{given, then, when, World};
use td_audit::compose::audit_compose;
use td_audit::cost::{estimate_cost, CostEstimate, DefaultPricing};
use td_audit::dockerfile::audit_dockerfile;
use td_audit::exposure::audit_exposure;
use td_audit::image::{audit_image, ImageInfo};
use td_audit::readiness::audit_readiness;
use td_audit::schema::{AuditReport, Grade, Severity};

#[derive(Debug, Default, World)]
struct AuditWorld {
    input: String,
    report: Option<AuditReport>,
    cost: Option<CostEstimate>,
}

impl AuditWorld {
    fn report(&self) -> &AuditReport {
        self.report.as_ref().expect("a checker must run before assertions")
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

fn fixture(subdir: &str, name: &str) -> String {
    let path = format!("{}/tests/fixtures/{subdir}/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("fixture {path} unreadable: {e}"))
}

// ---- Given -----------------------------------------------------------------

#[given(regex = r#"^the compose fixture "([^"]+)"$"#)]
async fn given_compose_fixture(world: &mut AuditWorld, name: String) {
    world.input = fixture("compose", &name);
}

#[given(regex = r#"^the dockerfile fixture "([^"]+)"$"#)]
async fn given_dockerfile_fixture(world: &mut AuditWorld, name: String) {
    world.input = fixture("dockerfile", &name);
}

#[given("a compose document that is not valid YAML")]
async fn given_invalid(world: &mut AuditWorld) {
    world.input = ":\n  not: valid: yaml: [".to_string();
}

#[given("an image reference that does not exist")]
async fn given_missing_image(world: &mut AuditWorld) {
    world.input = "trydirect/does-not-exist:latest".to_string();
}

// ---- When ------------------------------------------------------------------

#[when("I audit the compose")]
async fn when_audit_compose(world: &mut AuditWorld) {
    world.report = Some(audit_compose(&world.input));
}

#[when(regex = r#"^I run the "([a-z]+)" checker$"#)]
async fn when_run_checker(world: &mut AuditWorld, checker: String) {
    let report = match checker.as_str() {
        "compose" => audit_compose(&world.input),
        "exposure" => audit_exposure(&world.input),
        "dockerfile" => audit_dockerfile(&world.input),
        "readiness" => audit_readiness(&world.input),
        other => panic!("unknown checker: {other}"),
    };
    world.report = Some(report);
}

#[when("I estimate cost with the default pricing")]
async fn when_estimate_cost(world: &mut AuditWorld) {
    world.cost = Some(estimate_cost(&world.input, &DefaultPricing));
}

#[when("I inspect the image")]
async fn when_inspect_image(world: &mut AuditWorld) {
    // Missing-image path — resolved metadata says it does not exist.
    let info = ImageInfo {
        reference: world.input.clone(),
        exists: false,
        official: false,
        pinned: false,
        last_updated_days: None,
    };
    world.report = Some(audit_image(&info, &[]));
}

// ---- Then ------------------------------------------------------------------

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

#[then("a cheapest provider is returned")]
async fn then_cheapest(world: &mut AuditWorld) {
    assert!(world.cost.as_ref().unwrap().cheapest.is_some());
}

#[then("the quotes are ordered cheapest first")]
async fn then_ordered(world: &mut AuditWorld) {
    let q = &world.cost.as_ref().unwrap().quotes;
    assert!(q.windows(2).all(|w| w[0].monthly_usd <= w[1].monthly_usd));
}

#[tokio::main]
async fn main() {
    AuditWorld::cucumber()
        .fail_on_skipped()
        .run_and_exit("tests/features")
        .await;
}
