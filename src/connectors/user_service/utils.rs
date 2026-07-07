/// Maps a plan name/code returned by the User Service to a canonical tier name.
/// The User Service may return display names (e.g. "Team", "Pro", "Enterprise")
/// or plan codes (e.g. "plan-individual-monthly", "plan-pro-monthly").
fn normalize_plan_to_tier(plan: &str) -> String {
    match plan.to_lowercase().as_str() {
        // Canonical names
        "free" => "free",
        "basic" => "basic",
        "professional" | "pro" => "professional",
        "team" => "professional",
        "enterprise" => "enterprise",
        // Plan codes (legacy / from User Service)
        "plan-individual-monthly" => "enterprise",
        "plan-individual-annualy" => "enterprise",
        "plan-plus-monthly" => "professional",
        "plan-plus-annualy" => "professional",
        "plan-basic-monthly" => "basic",
        "plan-basic-annualy" => "basic",
        "plan-pro-monthly" | "plan-professional-monthly" => "professional",
        "plan-enterprise-monthly" | "plan-enterprise-yearly" => "enterprise",
        // Unknown — pass through as-is so the caller can decide
        other => other,
    }
    .to_string()
}

/// Helper function to determine if a plan tier can access a required plan
/// Hierarchy (lowest to highest): free < basic < professional < enterprise
pub(crate) fn is_plan_higher_tier(user_plan: &str, required_plan: &str) -> bool {
    let plan_hierarchy = vec!["free", "basic", "professional", "enterprise"];

    let user_tier = normalize_plan_to_tier(user_plan);
    let required_tier = normalize_plan_to_tier(required_plan);

    let user_level = plan_hierarchy.iter().position(|&p| p == user_tier);
    let required_level = plan_hierarchy.iter().position(|&p| p == required_tier);

    match (user_level, required_level) {
        (Some(user_level), Some(required_level)) => user_level >= required_level,
        // Fail closed if either plan is unknown
        _ => false,
    }
}
