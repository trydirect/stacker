# TODO: Plan Integration & Marketplace Payment for Stacker

## Context
Stacker needs to:
1. **List available plans** for UI display (from User Service)
2. **Validate user has required plan** before allowing deployment
3. **Initiate subscription flow** if user lacks required plan
4. **Process marketplace template purchases** (one-time or subscription-based verified pro stacks)
5. **Gating** deployments based on plan tier and template requirements

**Business Model**: Stop charging per deployment → Start charging per **managed server** ($10/mo) + **verified pro stack subscriptions**

Currently Stacker enforces `required_plan_name` on templates, but needs connectors to check actual user plan status and handle marketplace payments.

## Tasks

### 1. Enhance User Service Connector (if needed)
**File**: `app/<stacker-module>/connectors/user_service_connector.py` (in Stacker repo)

**Check if these methods exist**:
```python
def get_available_plans() -> list:
    """
    GET http://user:4100/server/user/plans/info
    
    Returns list of all plan definitions for populating admin forms
    """
    pass

def get_user_plan_info(user_token: str) -> dict:
    """
    GET http://user:4100/oauth_server/api/me
    Headers: Authorization: Bearer {user_token}
    
    Returns:
    {
        "plan": {
            "name": "plus",
            "date_end": "2026-01-30",
            "deployments_left": 8,
            "supported_stacks": {...}
        }
    }
    """
    pass

def user_has_plan(user_token: str, required_plan_name: str) -> bool:
    """
    Check if user's current plan meets or exceeds required_plan_name
    
    Uses PLANS_SENIORITY_ORDER: ["free", "basic", "plus", "individual"]
    """
    pass
```

**Implementation Note**: These should use the OAuth2 token that Stacker already has for the user.

### 2. Create Payment Service Connector
**File**: `app/<stacker-module>/connectors/payment_service_connector.py` (in Stacker repo)

**New connector** using `PaymentServiceClient` from try.direct.tools:
```python
from tools.common.v1 import PaymentServiceClient
from os import environ

class StackerPaymentConnector:
    def __init__(self):
        self.client = PaymentServiceClient(
            base_url=environ['URL_SERVER_PAYMENT'],
            auth_token=environ.get('STACKER_SERVICE_TOKEN')  # For service-to-service auth
        )
    
    def start_subscription(self, payment_method: str, plan_name: str, user_email: str, user_domain: str) -> dict:
        """
        Initiate subscription checkout for plan upgrade
        
        Returns:
        {
            'checkout_url': 'https://checkout.stripe.com/...',
            'session_id': 'cs_...',
            'payment_id': 123
        }
        """
        return self.client.create_subscription_checkout(
            payment_method=payment_method,
            plan_name=plan_name,
            user_data={
                'user_email': user_email,
                'user_domain': user_domain,
                'billing_first_name': '',  # Can prompt user or leave empty
                'billing_last_name': ''
            }
        )
    
    def purchase_marketplace_template(self, payment_method: str, template_id: str, user_email: str, user_domain: str) -> dict:
        """
        Initiate payment for verified pro stack from marketplace
        
        Args:
            template_id: marketplace template ID
            (Payment Service looks up template price)
        
        Returns:
        {
            'checkout_url': 'https://checkout.stripe.com/...',
            'session_id': 'cs_...',
            'payment_id': 123,
            'template_id': template_id
        }
        """
        return self.client.create_single_payment_checkout(
            payment_method=payment_method,
            stack_code=template_id,  # Use template_id as stack_code
            user_data={
                'user_email': user_email,
                'user_domain': user_domain,
                'template_id': template_id,
                'billing_first_name': '',
                'billing_last_name': ''
            }
        )
```

### 3. Add Billing Endpoints in Stacker API
**File**: `app/<stacker-module>/routes/billing.py` (new file in Stacker repo)

```python
from flask import Blueprint, request, jsonify
from .connectors.payment_service_connector import StackerPaymentConnector
from .connectors.user_service_connector import get_user_plan_info

billing_bp = Blueprint('billing', __name__)
payment_connector = StackerPaymentConnector()

@billing_bp.route('/billing/start', methods=['POST'])
def start_billing():
    """
    POST /billing/start
    Body: {
        "payment_method": "stripe" | "paypal",
        "plan_name": "basic" | "plus" | "individual",
        "user_email": "user@example.com",
        "user_domain": "try.direct"  # Or "dev.try.direct" for sandbox
    }
    
    Returns:
    {
        "checkout_url": "...",
        "session_id": "...",
        "payment_id": 123
    }
    """
    data = request.json
    result = payment_connector.start_subscription(
        payment_method=data['payment_method'],
        plan_name=data['plan_name'],
        user_email=data['user_email'],
        user_domain=data.get('user_domain', 'try.direct')
    )
    return jsonify(result)

@billing_bp.route('/billing/purchase-template', methods=['POST'])
def purchase_template():
    """
    POST /billing/purchase-template
    Body: {
        "payment_method": "stripe" | "paypal",
        "template_id": "uuid-of-marketplace-template",
        "user_email": "user@example.com",
        "user_domain": "try.direct"
    }
    
    Initiate payment for verified pro stack from marketplace (one-time or subscription).
    Payment Service looks up template pricing from user_service marketplace_templates table.
    
    Returns:
    {
        "checkout_url": "...",
        "session_id": "...",
        "payment_id": 123,
        "template_id": "..."
    }
    """
    data = request.json
    result = payment_connector.purchase_marketplace_template(
        payment_method=data['payment_method'],
        template_id=data['template_id'],
        user_email=data['user_email'],
        user_domain=data.get('user_domain', 'try.direct')
    )
    return jsonify(result)

@billing_bp.route('/billing/status', methods=['GET'])
def check_status():
    """
    GET /billing/status?user_token={token}
    
    Returns current user plan info
    """
    user_token = request.args.get('user_token')
    plan_info = get_user_plan_info(user_token)
    return jsonify(plan_info)
```

**Register blueprint** in main app:
```python
from .routes.billing import billing_bp
app.register_blueprint(billing_bp)
```

### 4. Update Deployment Validation & Marketplace Template Gating
**File**: `app/<stacker-module>/services/deployment_service.py` (or wherever deploy happens in Stacker)

**Before allowing deployment**:
```python
from .connectors.user_service_connector import user_has_plan, get_user_plan_info
from .connectors.payment_service_connector import StackerPaymentConnector

class DeploymentValidator:
    def validate_deployment(self, template, user_token, user_email):
        """
        Validate deployment eligibility:
        1. Check required plan for template type
        2. Check if marketplace template requires payment
        3. Block deployment if requirements not met
        """
        # Existing validation...
        
        # Plan requirement check
        required_plan = template.required_plan_name
        if required_plan:
            if not user_has_plan(user_token, required_plan):
                raise InsufficientPlanError(
                    f"This template requires '{required_plan}' plan or higher. "
                    f"Please upgrade at /billing/start"
                )
        
        # Marketplace verified pro stack check
        if template.is_from_marketplace and template.is_paid:
            # Check if user has purchased this template
            user_plan = get_user_plan_info(user_token)
            if template.id not in user_plan.get('purchased_templates', []):
                raise TemplateNotPurchasedError(
                    f"This verified pro stack requires payment. "
                    f"Please purchase at /billing/purchase-template"
                )
        
        # Continue with deployment...
```

**Frontend Integration** (Stacker UI):
```typescript
// If deployment blocked due to insufficient plan
if (error.code === 'INSUFFICIENT_PLAN') {
  // Show upgrade modal
  <UpgradeModal
    requiredPlan={error.required_plan}
    onUpgrade={() => {
      // Call Stacker backend /billing/start
      fetch('/billing/start', {
        method: 'POST',
        body: JSON.stringify({
          payment_method: 'stripe',
          plan_name: error.required_plan,
          user_email: currentUser.email,
          user_domain: window.location.hostname
        })
      })
      .then(res => res.json())
      .then(data => {
        // Redirect to payment provider
        window.location.href = data.checkout_url;
      });
    }}
  />
}

// If deployment blocked due to unpaid marketplace template
if (error.code === 'TEMPLATE_NOT_PURCHASED') {
  <PurchaseTemplateModal
    templateId={error.template_id}
    templateName={error.template_name}
    price={error.price}
    onPurchase={() => {
      fetch('/billing/purchase-template', {
        method: 'POST',
        body: JSON.stringify({
          payment_method: 'stripe',
          template_id: error.template_id,
          user_email: currentUser.email,
          user_domain: window.location.hostname
        })
      })
      .then(res => res.json())
      .then(data => {
        window.location.href = data.checkout_url;
      });
    }}
  />
}
```

## Environment Variables Needed (Stacker)
Add to Stacker's `.env`:
```bash
# Payment Service
URL_SERVER_PAYMENT=http://payment:8000/

# Service-to-service auth token (get from User Service admin)
STACKER_SERVICE_TOKEN=<bearer-token-from-user-service>

# Or use OAuth2 client credentials (preferred)
STACKER_CLIENT_ID=<from-user-service>
STACKER_CLIENT_SECRET=<from-user-service>
```
// If deployment blocked due to insufficient plan
if (error.code === 'INSUFFICIENT_PLAN') {
  // Show upgrade modal
  <UpgradeModal
    requiredPlan={error.required_plan}
    onUpgrade={() => {
      // Call Stacker backend /billing/start
      fetch('/billing/start', {
        method: 'POST',
        body: JSON.stringify({
          payment_method: 'stripe',
          plan_name: error.required_plan,
          user_email: currentUser.email,
          user_domain: window.location.hostname
        })
      })
      .then(res => res.json())
      .then(data => {
        // Redirect to payment provider
        window.location.href = data.checkout_url;
      });
    }}
  />
}
```

## Testing Checklist
- [ ] User Service connector returns plan list
- [ ] User Service connector checks user plan status
- [ ] User Service connector returns user plan with `purchased_templates` field
- [ ] Payment connector creates Stripe checkout session (plan upgrade)
- [ ] Payment connector creates PayPal checkout session (plan upgrade)
- [ ] Payment connector creates Stripe session for marketplace template purchase
- [ ] Payment connector creates PayPal session for marketplace template purchase
- [ ] Deployment blocked if insufficient plan (returns INSUFFICIENT_PLAN error)
- [ ] Deployment blocked if marketplace template not purchased (returns TEMPLATE_NOT_PURCHASED error)
- [ ] Deployment proceeds for free templates with free plan
- [ ] Deployment proceeds for verified pro templates after purchase
- [ ] `/billing/start` endpoint returns valid Stripe checkout URL
- [ ] `/billing/start` endpoint returns valid PayPal checkout URL
- [ ] `/billing/purchase-template` endpoint returns valid checkout URL
- [ ] Redirect to Stripe payment works
- [ ] Redirect to PayPal payment works
- [ ] Webhook from Payment Service activates plan in User Service
- [ ] Webhook from Payment Service marks template as purchased in User Service
- [ ] After plan upgrade payment, deployment proceeds successfully
- [ ] After template purchase, user can deploy that template
- [ ] Marketplace template fields (`is_from_marketplace`, `is_paid`, `price`) available in Stacker

## Coordination
**Dependencies**:
1. ✅ try.direct.tools: Add `PaymentServiceClient` (TODO.md created)
2. ✅ try.direct.payment.service: Endpoints exist (no changes needed)
3. ✅ try.direct.user.service: Plan management + marketplace webhooks (minimal changes for `purchased_templates`)
4. ⏳ Stacker: Implement connectors + billing endpoints + marketplace payment flows (THIS TODO)

**Flow After Implementation**:

**Plan Upgrade Flow**:
```
User clicks "Deploy premium template" in Stacker
  → Stacker checks user plan via User Service connector
  → If insufficient (e.g., free plan trying plus template):
      → Show "Upgrade Required" modal
      → User clicks "Upgrade Plan"
      → Stacker calls /billing/start
      → Returns Stripe/PayPal checkout URL + session_id
      → User redirected to payment provider
      → User completes payment
      → Payment Service webhook → User Service (plan activated, user_plans updated)
      → User returns to Stacker
      → Stacker re-checks plan (now sufficient)
      → Deployment proceeds
```

**Marketplace Template Purchase Flow**:
```
User deploys verified pro stack (paid template from marketplace)
  → Stacker checks if template.is_paid and template.is_from_marketplace
  → Queries user's purchased_templates list from User Service
  → If not in list:
      → Show "Purchase Stack" modal with price
      → User clicks "Purchase"
      → Stacker calls /billing/purchase-template
      → Returns Stripe/PayPal checkout URL + payment_id
      → User completes payment
      → Payment Service webhook → User Service (template marked purchased)
      → User returns to Stacker
      → Stacker re-checks purchased_templates
      → Deployment proceeds
```
      → User returns to Stacker
      → Stacker re-checks plan (now sufficient)
      → Deployment proceeds
```

## Notes
- **DO NOT store plans in Stacker database** - always query User Service
- **DO NOT call Stripe/PayPal directly** - always go through Payment Service
- Payment Service handles all webhook logic and User Service updates
- Stacker only needs to validate and redirect
