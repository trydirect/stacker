# TryDirect Payment Service - AI Coding Guidelines

## Project Overview
Django-based payment gateway service for TryDirect platform that handles single payments and subscriptions via PayPal, Stripe, Coinbase, and Ethereum. Runs as a containerized microservice with HashiCorp Vault for secrets management.

**Important**: This is an internal service with no public routes - all endpoints are accessed through internal network only. No authentication is implemented as the service is not exposed to the internet.

### Testing Payments
Use curl with Bearer token (see [readme.md](readme.md) for examples):
```bash
export TOKEN=<token>
curl -X POST "http://localhost:8000/single_payment/stripe/" \
  -H "Content-type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  --data '{"variant": "stripe", "description": "matomo", "total": 55, ...}'
```


### URL Patterns
- `/single_payment/{provider}/` - one-time payments
- `/subscribe_to_plan/{provider}/` - create subscription
- `/webhooks/{provider}/` - provider callbacks
- `/cancel_subscription/` - unified cancellation endpoint

PayPal
--
curl -X POST "http://localhost:8000/single_payment/paypal/" -H "Content-type: application/json" -H "Authorization: Bearer $TOKEN" --data '{"variant": "paypal", "description": "matomo", "total": 55, "tax": 0.0, "currency": "USD", "delivery": 0.0, "billing_first_name": "", "billing_last_name":  "", "billing_address_1": "", "billing_address_2": "", "billing_city": "", "billing_postcode": "", "billing_country_code": "", "billing_country_area": "", "billing_email": "info@try.direct", "transaction_id": 0, "common_domain": "sample.com", "plan_name": "SinglePayment", "installation_id": 13284, "user_domain":"https://dev.try.direct"}'

Stripe
--
curl -X POST "http://localhost:8000/single_payment/stripe/" -H "Content-type: application/json" -H "Authorization: Bearer $TOKEN" --data '{"variant": "stripe", "description": "matomo", "total": 55, "tax": 0.0, "currency": "USD", "delivery": 0.0, "billing_first_name": "", "billing_last_name":  "", "billing_address_1": "", "billing_address_2": "", "billing_city": "", "billing_postcode": "", "billing_country_code": "", "billing_country_area": "", "billing_email": "info@try.direct", "transaction_id": 0, "common_domain": "sample.com", "plan_name": "SinglePayment", "installation_id": 13284, "installation_info": {"commonDomain": "sample.com", "domainList": {}, "ssl": "letsencrypt", "vars": [{"code": "matomo", "title": "Matomo", "_id": 97, "versions": [{"version": "5.2.1", "name": "Matomo", "dependencies": [473, 69, 74], "excluded": [], "masters": [], "disabled": false, "_id": 208}], "selectedVersion": {"version": "5.2.1", "name": "Matomo", "dependencies": [473, 69, 74], "excluded": [], "masters": [], "disabled": false, "_id": 208, "tag": "unstable"}, "ansible_var": "matomo", "group_code": null}, {"code": "mysql", "title": "MySQL", "_id": 1, "versions": [{"version": "8.0", "name": "8.0", "dependencies": [], "excluded": [], "masters": [208], "disabled": false, "_id": 473}], "selectedVersion": {"version": "8.0", "name": "8.0", "dependencies": [], "excluded": [], "masters": [208], "disabled": false, "_id": 473, "tag": "8.0"}, "ansible_var": null, "group_code": "database"}, {"code": "rabbitmq", "title": "RabbitMQ", "_id": 42, "versions": [{"version": "3-management", "name": "3-management", "dependencies": [], "excluded": [], "masters": [208], "disabled": false, "_id": 69}], "selectedVersion": {"version": "3-management", "name": "3-management", "dependencies": [], "excluded": [], "masters": [208], "disabled": false, "_id": 69, "tag": "3-management"}, "ansible_var": null, "group_code": null}, {"code": "redis", "title": "Redis", "_id": 45, "versions": [{"version": "latest", "name": "latest", "dependencies": [], "excluded": [], "masters": [208], "disabled": false, "_id": 74}], "selectedVersion": {"version": "latest", "name": "latest", "dependencies": [], "excluded": [], "masters": [208], "disabled": false, "_id": 74, "tag": "latest"}, "ansible_var": null, "group_code": null}], "integrated_features": ["nginx_feature", "fail2ban"], "extended_features": [], "subscriptions": [], "form_app": [], "region": "fsn1", "zone": null, "server": "cx22", "os": "ubuntu-20.04", "disk_type": "pd-standart", "servers_count": 3, "save_token": false, "cloud_token": "***", "provider": "htz", "stack_code": "matomo", "selected_plan": null, "version": "latest", "payment_type": "single", "payment_method": "paypal", "currency": "USD", "installation_id": 13284, "user_domain": "https://dev.try.direct/"}}'