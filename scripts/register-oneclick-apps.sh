#!/bin/bash
# register-oneclick-apps.sh
#
# Registers apps from awesome-selfhosted-stacker as deployment_daily
# templates using the Stacker CLI + API.
#
# Usage: ./register-oneclick-apps.sh <stacker_url> <user_token>
#
# Flow per app:
#   1. cd into app directory
#   2. stacker submit --plan-type deployment_daily --price 0
#   3. Approve via API
#   4. Update daily_rate/monthly_cap via SQL

set -euo pipefail

STACKER_URL="${1:?Usage: $0 <stacker_url> <user_token>}"
USER_TOKEN="${2:?Usage: $0 <stacker_url> <user_token>}"
REPO_URL="https://github.com/trydirect/awesome-selfhosted-stacker.git"
CLONE_DIR="/tmp/oneclick-register-$$"

# Pricing: slug → daily_rate|monthly_cap
declare -A PRICING=(
    ["aptabase"]="0.27|8.00"
    ["linkding"]="0.27|8.00"
    ["mealie"]="0.27|8.00"
    ["freshrss"]="0.27|8.00"
    ["stirling-pdf"]="0.27|8.00"
    ["appsmith"]="0.86|26.00"
    ["floci"]="0.86|26.00"
    ["zitadel"]="0.86|26.00"
    ["supabase"]="0.86|26.00"
    ["immich"]="0.86|26.00"
    ["stackpilot"]="0.86|26.00"
    ["supabase-posthog"]="1.67|50.00"
    ["ai-knowledge-base"]="1.67|50.00"
    ["ai-workflows-v2"]="1.67|50.00"
    ["private-sovereign-ai"]="3.30|99.00"
)

trap "rm -rf $CLONE_DIR" EXIT

echo "==> Cloning repo..."
git clone --depth 1 "$REPO_URL" "$CLONE_DIR" 2>/dev/null
cd "$CLONE_DIR/stacker-projects"

# Check stacker CLI is available
if ! command -v stacker &>/dev/null; then
    echo "ERROR: 'stacker' CLI not found in PATH"
    exit 1
fi

# Configure CLI credentials
export STACKER_URL
export STACKER_TOKEN="$USER_TOKEN"

SUCCESS=0
FAILED=0
SKIPPED=0

for app_dir in */; do
    app_name="${app_dir%/}"
    
    if [ ! -f "$app_dir/stacker.yml" ]; then
        echo "SKIP: $app_name (no stacker.yml)"
        SKIPPED=$((SKIPPED + 1))
        continue
    fi

    # Check if pricing is defined
    if [ -z "${PRICING[$app_name]:-}" ]; then
        echo "SKIP: $app_name (no pricing defined)"
        SKIPPED=$((SKIPPED + 1))
        continue
    fi

    pricing="${PRICING[$app_name]}"
    daily_rate=$(echo "$pricing" | cut -d'|' -f1)
    monthly_cap=$(echo "$pricing" | cut -d'|' -f2)

    echo ""
    echo "==> Processing: $app_name (\$$daily_rate/day, \$$monthly_cap/mo cap)"

    cd "$CLONE_DIR/stacker-projects/$app_name"

    # Step 1: Submit via CLI
    echo "  Submitting via stacker CLI..."
    if ! stacker submit \
        --plan-type deployment_daily \
        --price 0 \
        --category "Self-Hosted" \
        2>/dev/null; then
        echo "  WARN: stacker submit failed, trying API directly..."
    fi

    # Step 2: Get template ID by slug
    echo "  Looking up template..."
    template_response=$(curl -s \
        "$STACKER_URL/api/v1/marketplace/templates?slug=$app_name" \
        -H "Authorization: Bearer $USER_TOKEN" 2>/dev/null)

    template_id=$(echo "$template_response" | python3 -c "
import sys, json
try:
    data = json.load(sys.stdin)
    templates = data.get('items', data.get('_items', []))
    if templates:
        print(templates[0]['id'])
except: pass
" 2>/dev/null)

    if [ -z "$template_id" ]; then
        echo "  FAIL: Could not find template for $app_name"
        FAILED=$((FAILED + 1))
        continue
    fi

    echo "  Template ID: $template_id"

    # Step 3: Approve via API
    echo "  Approving template..."
    approve_response=$(curl -s -w "\n%{http_code}" -X POST \
        "$STACKER_URL/api/v1/marketplace/templates/$template_id/approve" \
        -H "Authorization: Bearer $USER_TOKEN" \
        -H "Content-Type: application/json" 2>/dev/null)

    approve_code=$(echo "$approve_response" | tail -1)
    if [ "$approve_code" = "200" ]; then
        echo "  Approved"
    else
        echo "  WARN: Approve returned HTTP $approve_code (may already be approved)"
    fi

    # Step 4: Update pricing via admin API
    echo "  Setting deployment_daily pricing..."
    pricing_response=$(curl -s -w "\n%{http_code}" -X PATCH \
        "$STACKER_URL/api/v1/marketplace/templates/$template_id/pricing" \
        -H "Authorization: Bearer $USER_TOKEN" \
        -H "Content-Type: application/json" \
        -d "{
            \"billing_cycle\": \"deployment_daily\",
            \"price\": 0,
            \"daily_rate\": $daily_rate,
            \"monthly_cap\": $monthly_cap,
            \"currency\": \"USD\"
        }" 2>/dev/null)

    pricing_code=$(echo "$pricing_response" | tail -1)
    if [ "$pricing_code" = "200" ]; then
        echo "  Pricing set: \$$daily_rate/day, \$$monthly_cap/mo cap"
    else
        echo "  WARN: Pricing update returned HTTP $pricing_code"
    fi

    echo "  DONE: $app_name"
    SUCCESS=$((SUCCESS + 1))

    cd "$CLONE_DIR/stacker-projects"
done

echo ""
echo "==> Summary: $SUCCESS registered, $FAILED failed, $SKIPPED skipped"
