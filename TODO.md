# TODO: Stacker Marketplace Payment Integration

## Context
Per [PAYMENT_MODEL.md](/PAYMENT_MODEL.md), Stacker now sends webhooks to User Service when templates are published/updated. User Service owns the `products` table for monetization, while Stacker owns `stack_template` (template definitions only).

Stacker responsibilities:
1. **Maintain `stack_template` table** (template definitions, no pricing/monetization)
2. **Send webhook to User Service** when template status changes (approved, updated, rejected)
3. **Query User Service** for product information (pricing, vendor, etc.)
4. **Validate deployments** against User Service product ownership

## Tasks

### 1. Create User Service Connector
**File**: `app/<stacker-module>/connectors/user_service_connector.py` (in Stacker repo)

**Required methods**:
```python
class UserServiceConnector:
    def get_user_profile(self, user_token: str) -> dict:
        """
        GET http://user:4100/oauth_server/api/me
        Headers: Authorization: Bearer {user_token}
        
        Returns:
        {
            "email": "user@example.com",
            "plan": {
                "name": "plus",
                "date_end": "2026-01-30"
            },
            "products": [
                {
                    "product_id": "uuid",
                    "product_type": "template",
                    "code": "ai-agent-stack",
                    "external_id": 12345,  # stack_template.id from Stacker
                    "name": "AI Agent Stack",
                    "price": "99.99",
                    "owned_since": "2025-01-15T..."
                }
            ]
        }
        """
        pass
    
    def get_template_product(self, stack_template_id: int) -> dict:
        """
        GET http://user:4100/api/1.0/products?external_id={stack_template_id}&product_type=template
        
        Returns product info for a marketplace template (pricing, vendor, etc.)
        """
        pass
    
    def user_owns_template(self, user_token: str, stack_template_id: int) -> bool:
        """
        Check if user has purchased/owns this marketplace template
        """
        profile = self.get_user_profile(user_token)
        return any(p['external_id'] == stack_template_id and p['product_type'] == 'template' 
                   for p in profile.get('products', []))
```

**Implementation Note**: Use OAuth2 token that Stacker already has for the user.

### 2. Create Webhook Sender to User Service (Marketplace Sync)
**File**: `app/<stacker-module>/webhooks/marketplace_webhook.py` (in Stacker repo)

**When template status changes** (approved, updated, rejected):
```python
import requests
from os import environ

class MarketplaceWebhookSender:
    """
    Send template sync webhooks to User Service
    Mirrors PAYMENT_MODEL.md Flow 3: Stacker template changes → User Service products
    """
    
    def send_template_approved(self, stack_template: dict, vendor_user: dict):
        """
        POST http://user:4100/marketplace/sync
        
        Body:
        {
            "action": "template_approved",
            "stack_template_id": 12345,
            "external_id": 12345,  # Same as stack_template_id
            "code": "ai-agent-stack-pro",
            "name": "AI Agent Stack Pro",
            "description": "Advanced AI agent deployment...",
            "price": 99.99,
            "billing_cycle": "one_time",  # or "monthly"
            "currency": "USD",
            "vendor_user_id": 456,
            "vendor_name": "John Doe"
        }
        """
        headers = {'Authorization': f'Bearer {self.get_service_token()}'}
        
        payload = {
            'action': 'template_approved',
            'stack_template_id': stack_template['id'],
            'external_id': stack_template['id'],
            'code': stack_template.get('code'),
            'name': stack_template.get('name'),
            'description': stack_template.get('description'),
            'price': stack_template.get('price'),
            'billing_cycle': stack_template.get('billing_cycle', 'one_time'),
            'currency': stack_template.get('currency', 'USD'),
            'vendor_user_id': vendor_user['id'],
            'vendor_name': vendor_user.get('full_name', vendor_user.get('email'))
        }
        
        response = requests.post(
            f"{environ['URL_SERVER_USER']}/marketplace/sync",
            json=payload,
            headers=headers
        )
        
        if response.status_code != 200:
            raise Exception(f"Webhook send failed: {response.text}")
        
        return response.json()
    
    def send_template_updated(self, stack_template: dict, vendor_user: dict):
        """Send template updated webhook (same format as approved)"""
        payload = {...}
        payload['action'] = 'template_updated'
        # Send like send_template_approved()
    
    def send_template_rejected(self, stack_template: dict):
        """
        Notify User Service to deactivate product
        
        Body:
        {
            "action": "template_rejected",
            "stack_template_id": 12345
        }
        """
        headers = {'Authorization': f'Bearer {self.get_service_token()}'}
        
        payload = {
            'action': 'template_rejected',
            'stack_template_id': stack_template['id']
        }
        
        response = requests.post(
            f"{environ['URL_SERVER_USER']}/marketplace/sync",
            json=payload,
            headers=headers
        )
        
        return response.json()
    
    @staticmethod
    def get_service_token() -> str:
        """Get Bearer token for service-to-service communication"""
        # Option 1: Use static bearer token
        return environ.get('STACKER_SERVICE_TOKEN')
        
        # Option 2: Use OAuth2 client credentials flow (preferred)
        # See User Service `.github/copilot-instructions.md` for setup
```

**Integration points** (where to call webhook sender):

1. **When template is approved by admin**:
```python
def approve_template(template_id: int):
    template = StackTemplate.query.get(template_id)
    vendor = User.query.get(template.created_by_user_id)
    template.status = 'approved'
    db.session.commit()
    
    # Send webhook to User Service to create product
    webhook_sender = MarketplaceWebhookSender()
    webhook_sender.send_template_approved(template.to_dict(), vendor.to_dict())
```

2. **When template is updated**:
```python
def update_template(template_id: int, updates: dict):
    template = StackTemplate.query.get(template_id)
    template.update(updates)
    db.session.commit()
    
    if template.status == 'approved':
        vendor = User.query.get(template.created_by_user_id)
        webhook_sender = MarketplaceWebhookSender()
        webhook_sender.send_template_updated(template.to_dict(), vendor.to_dict())
```

3. **When template is rejected**:
```python
def reject_template(template_id: int):
    template = StackTemplate.query.get(template_id)
    template.status = 'rejected'
    db.session.commit()
    
    webhook_sender = MarketplaceWebhookSender()
    webhook_sender.send_template_rejected(template.to_dict())
```

### 3. Add Deployment Validation
**File**: `app/<stacker-module>/services/deployment_service.py` (update existing)

**Before allowing deployment, validate**:
```python
from .connectors.user_service_connector import UserServiceConnector

class DeploymentValidator:
    def validate_marketplace_template(self, stack_template: dict, user_token: str):
        """
        Check if user can deploy this marketplace template
        
        If template has a product in User Service:
        - Check if user owns product (in user_products table)
        - If not owned, block deployment
        """
        connector = UserServiceConnector()
        
        # If template is not marketplace template, allow deployment
        if not stack_template.get('is_from_marketplace'):
            return True
        
        # Check if template has associated product
        template_id = stack_template['id']
        product_info = connector.get_template_product(template_id)
        
        if not product_info:
            # No product = free marketplace template, allow deployment
            return True
        
        # Check if user owns this template product
        user_owns = connector.user_owns_template(user_token, template_id)
        
        if not user_owns:
            raise TemplateNotPurchasedError(
                f"This verified pro stack requires purchase. "
                f"Price: ${product_info.get('price')}. "
                f"Please purchase from User Service."
            )
        
        return True
```

**Integrate into deployment flow**:
```python
def start_deployment(template_id: int, user_token: str):
    template = StackTemplate.query.get(template_id)
    
    # Validate permission to deploy this template
    validator = DeploymentValidator()
    validator.validate_marketplace_template(template.to_dict(), user_token)
    
    # Continue with deployment...
```

## Environment Variables Needed (Stacker)
Add to Stacker's `.env`:
```bash
# User Service
URL_SERVER_USER=http://user:4100/

# Service-to-service auth token (for webhook sender)
STACKER_SERVICE_TOKEN=<bearer-token-from-user-service>

# Or use OAuth2 client credentials (preferred)
STACKER_CLIENT_ID=<from-user-service>
STACKER_CLIENT_SECRET=<from-user-service>
```

## Testing Checklist

### Unit Tests
- [ ] `test_user_service_connector.py`:
  - [ ] `get_user_profile()` returns user with products list
  - [ ] `get_template_product()` returns product info
  - [ ] `user_owns_template()` returns correct boolean
- [ ] `test_marketplace_webhook_sender.py`:
  - [ ] `send_template_approved()` sends correct webhook payload
  - [ ] `send_template_updated()` sends correct webhook payload
  - [ ] `send_template_rejected()` sends correct webhook payload
  - [ ] `get_service_token()` returns valid bearer token
- [ ] `test_deployment_validator.py`:
  - [ ] `validate_marketplace_template()` allows free templates
  - [ ] `validate_marketplace_template()` allows user-owned paid templates
  - [ ] `validate_marketplace_template()` blocks non-owned paid templates
  - [ ] Raises `TemplateNotPurchasedError` with correct message

### Integration Tests
- [ ] `test_template_approval_flow.py`:
  - [ ] Admin approves template in Stacker
  - [ ] Webhook sent to User Service `/marketplace/sync`
  - [ ] User Service creates product
  - [ ] `/oauth_server/api/me` includes new product
- [ ] `test_template_update_flow.py`:
  - [ ] Vendor updates template in Stacker
  - [ ] Webhook sent to User Service
  - [ ] Product updated in User Service
- [ ] `test_template_rejection_flow.py`:
  - [ ] Admin rejects template
  - [ ] Webhook sent to User Service
  - [ ] Product deactivated in User Service
- [ ] `test_deployment_validation_flow.py`:
  - [ ] User can deploy free marketplace template
  - [ ] User cannot deploy paid template without purchase
  - [ ] User can deploy paid template after product purchase
  - [ ] Correct error messages in each scenario

### Manual Testing
- [ ] Stacker can query User Service `/oauth_server/api/me` (with real user token)
- [ ] Stacker connector returns user profile with products list
- [ ] Approve template in Stacker admin → webhook sent to User Service
- [ ] User Service `/marketplace/sync` creates product
- [ ] Product appears in `/api/1.0/products` endpoint
- [ ] Deployment validation blocks unpurchased paid templates
- [ ] Deployment validation allows owned paid templates
- [ ] All environment variables configured correctly

## Coordination

**Dependencies**:
1. ✅ User Service - `/marketplace/sync` webhook endpoint (created in User Service TODO)
2. ✅ User Service - `products` + `user_products` tables (created in User Service TODO)
3. ⏳ Stacker - User Service connector + webhook sender (THIS TODO)
4. ✅ Payment Service - No changes needed (handles all webhooks same way)

**Service Interaction Flow**:

```
Vendor Creates Template in Stacker
  ↓
Admin Approves in Stacker
  ↓
Stacker calls MarketplaceWebhookSender.send_template_approved()
  ↓
POST http://user:4100/marketplace/sync
  {
    "action": "template_approved",
    "stack_template_id": 12345,
    "price": 99.99,
    "vendor_user_id": 456,
    ...
  }
  ↓
User Service creates `products` row
  (product_type='template', external_id=12345, vendor_id=456, price=99.99)
  ↓
Template now available in User Service `/api/1.0/products?product_type=template`
  ↓
Blog queries User Service for marketplace templates
  ↓
User views template in marketplace, clicks "Deploy"
  ↓
User pays (Payment Service handles all payment flows)
  ↓
Payment Service webhook → User Service (adds row to `user_products`)
  ↓
Stacker queries User Service `/oauth_server/api/me`
  ↓
User Service returns products list (includes newly purchased template)
  ↓
DeploymentValidator.validate_marketplace_template() checks ownership
  ↓
Deployment proceeds (user owns product)
```

## Notes

**Architecture Decisions**:
1. Stacker only sends webhooks to User Service (no bi-directional queries)
2. User Service owns monetization logic (products table)
3. Payment Service forwards webhooks to User Service (same handler for all product types)
4. `stack_template.id` (Stacker) links to `products.external_id` (User Service) via webhook
5. Deployment validation queries User Service for product ownership

**Key Points**:
- DO NOT store pricing in Stacker `stack_template` table
- DO NOT create products table in Stacker (they're in User Service)
- DO send webhooks to User Service when template status changes
- DO use Bearer token for service-to-service auth in webhooks
- Webhook sender is simpler than Stacker querying User Service (one-way communication)

## Timeline Estimate

- Phase 1 (User Service connector): 1-2 hours
- Phase 2 (Webhook sender): 1-2 hours
- Phase 3 (Deployment validation): 1-2 hours
- Phase 4 (Testing): 3-4 hours
- **Total**: 6-10 hours (~1 day)

## Reference Files
- [PAYMENT_MODEL.md](/PAYMENT_MODEL.md) - Architecture
- [try.direct.user.service/TODO.md](try.direct.user.service/TODO.md) - User Service implementation
- [try.direct.tools/TODO.md](try.direct.tools/TODO.md) - Shared utilities
- [blog/TODO.md](blog/TODO.md) - Frontend marketplace UI

