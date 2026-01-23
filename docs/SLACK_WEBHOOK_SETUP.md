# Slack Webhook Configuration for AI Support Escalation

This document describes how to configure Slack webhooks for the AI assistant's support escalation feature.

## Overview

When users interact with the TryDirect AI assistant and the AI cannot resolve their issue, it can escalate to human support via Slack. This creates a structured message in your support channel with:

- User information (email, user ID)
- Issue description
- Urgency level (🟢 low, 🟡 medium, 🔴 high/critical)
- Deployment context (if applicable)
- Conversation summary
- AI troubleshooting steps already attempted

## Setup Instructions

### 1. Create a Slack App

1. Go to [Slack API: Apps](https://api.slack.com/apps)
2. Click **"Create New App"**
3. Choose **"From scratch"**
4. Name it: `TryDirect AI Escalations`
5. Select your workspace

### 2. Configure Incoming Webhooks

1. In your app settings, go to **"Incoming Webhooks"**
2. Toggle **"Activate Incoming Webhooks"** to ON
3. Click **"Add New Webhook to Workspace"**
4. Select the channel for support escalations (e.g., `#trydirectflow` or `#support-escalations`)
5. Click **"Allow"**
6. Copy the **Webhook URL** – do **not** commit the real URL. Use placeholders in docs/examples, e.g.:
  ```
  https://example.com/slack-webhook/REPLACE_ME
  ```

### 3. Configure Environment Variables

Add these to your `.env` file (or Vault for production):

```bash
# Slack Support Escalation Webhook
SLACK_SUPPORT_WEBHOOK_URL=<SLACK_INCOMING_WEBHOOK_URL>
SLACK_SUPPORT_CHANNEL=#trydirectflow

# Optional: Different webhook for critical issues
SLACK_CRITICAL_WEBHOOK_URL=<SLACK_CRITICAL_WEBHOOK_URL>
```

### 4. Production Deployment

For production, store the webhook URL in HashiCorp Vault:

```bash
# Store in Vault
vault kv put secret/stacker/slack \
  support_webhook_url="<SLACK_INCOMING_WEBHOOK_URL>" \
  support_channel="#trydirectflow"
```

Update `stacker/config.hcl` to include Slack secrets:

```hcl
secret {
  path   = "secret/stacker/slack"
  no_prefix = true
  format = "SLACK_{{ key }}"
}
```

### 5. Test the Integration

Run the integration test:

```bash
cd stacker
SLACK_SUPPORT_WEBHOOK_URL="<SLACK_INCOMING_WEBHOOK_URL>" \
  cargo test test_slack_webhook_connectivity -- --ignored
```

Or use curl to send a test message:

```bash
curl -X POST "https://example.com/slack-webhook/REPLACE_ME" \
  -H "Content-Type: application/json" \
  -d '{
    "blocks": [
      {
        "type": "header",
        "text": {
          "type": "plain_text",
          "text": "🧪 Test Escalation",
          "emoji": true
        }
      },
      {
        "type": "section",
        "text": {
          "type": "mrkdwn",
          "text": "This is a test message from TryDirect AI escalation setup."
        }
      }
    ]
  }'
```

## Message Format

The AI sends Block Kit formatted messages with the following structure:

```
┌────────────────────────────────────────┐
│ 🔴 Support Escalation                  │
├────────────────────────────────────────┤
│ User: user@example.com                 │
│ Urgency: critical                      │
├────────────────────────────────────────┤
│ Reason:                                │
│ User's deployment is failing with      │
│ database connection timeout errors.    │
│ Already tried: restart container,      │
│ check logs, verify credentials.        │
├────────────────────────────────────────┤
│ Deployment ID: 12345                   │
│ Status: error                          │
├────────────────────────────────────────┤
│ Conversation Summary:                  │
│ User reported slow website. Checked    │
│ container health (OK), logs showed DB  │
│ timeouts. Suggested increasing pool    │
│ size but user needs admin access.      │
├────────────────────────────────────────┤
│ Escalated via AI Assistant • ID: xyz   │
└────────────────────────────────────────┘
```

## Urgency Levels

| Level | Emoji | Description | SLA Target |
|-------|-------|-------------|------------|
| `low` | 🟢 | General question, feature request | 24-48 hours |
| `normal` | 🟢 | Needs help, no service impact | 24 hours |
| `high` | 🟡 | Service degraded, some impact | 4 hours |
| `critical` | 🔴 | Service down, production issue | 1 hour |

## Channel Recommendations

Consider creating dedicated channels:

- `#support-escalations` - All AI escalations
- `#support-critical` - Critical/urgent issues only (separate webhook)
- `#support-after-hours` - Route to on-call during off hours

## Monitoring & Alerts

### Slack App Metrics

Monitor these in your Slack app dashboard:
- Total messages sent
- Failed delivery attempts
- Rate limit hits

### Application Logging

The Stacker service logs all escalations:

```
INFO user_id=123 escalation_id=abc urgency=high deployment_id=456 slack_success=true "Support escalation created via MCP"
```

Query logs to track escalation patterns:
- Most common escalation reasons
- User escalation frequency
- Time-to-resolution (correlate with support tickets)

## Troubleshooting

### Webhook Not Working

1. **Check URL format**: Must start with `https://hooks.slack.com/services/`
2. **Verify channel permissions**: Bot must be added to the channel
3. **Test connectivity**: Use curl to send a test message
4. **Check logs**: Look for `Slack webhook returned error` in Stacker logs

### Rate Limiting

Slack has rate limits for incoming webhooks:
- 1 message per second per webhook
- Burst: up to 10 messages quickly, then throttled

If hitting limits:
- Implement request queuing
- Use multiple webhooks for different urgency levels
- Batch low-priority escalations

### Message Not Appearing

1. Check if message is in a thread (search for escalation ID)
2. Verify bot is in the channel: `/invite @TryDirect AI Escalations`
3. Check channel notification settings

## Security Considerations

- **Never expose webhook URLs** in client-side code or logs
- **Rotate webhooks periodically** (regenerate in Slack app settings)
- **Monitor for abuse**: Track unusual escalation patterns
- **Redact PII**: Ensure conversation summaries don't include passwords/tokens

## Related Files

| File | Purpose |
|------|---------|
| [stacker/src/mcp/tools/support.rs](stacker/src/mcp/tools/support.rs) | Escalation tool implementation |
| [stacker/tests/mcp_integration.rs](stacker/tests/mcp_integration.rs) | Integration tests |
| [env.dist](env.dist) | Environment variable template |
