/// Helper function to determine if a plan tier can access a required plan
/// Basic idea: enterprise >= professional >= basic
pub(crate) fn is_plan_upgrade(user_plan: &str, required_plan: &str) -> bool {
    let plan_hierarchy = vec!["basic", "professional", "enterprise"];

    let user_level = plan_hierarchy
        .iter()
        .position(|&p| p == user_plan)
        .unwrap_or(0);
    let required_level = plan_hierarchy
        .iter()
        .position(|&p| p == required_plan)
        .unwrap_or(0);

    user_level > required_level
}
