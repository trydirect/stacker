# Support Team Escalation Handling Guide

> **Version**: 1.0  
> **Last Updated**: January 22, 2026  
> **Audience**: TryDirect Support Team

---

## Overview

The TryDirect AI Assistant can escalate issues to human support when it cannot resolve a user's problem. This guide explains how escalations work, what information you'll receive, and how to handle them effectively.

---

## Escalation Channels

### 1. Slack (`#trydirectflow`)

**Primary channel for all AI escalations.**

When the AI escalates, you'll receive a message in `#trydirectflow`:

```
🆘 AI Escalation Request
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
User: john.doe@example.com
User ID: 12345
Deployment: abc123def456 (Mautic stack)
Priority: medium

Issue Summary:
Container "mautic" keeps crashing after restart. AI attempted
log analysis and found PHP memory exhaustion errors but
automated fixes did not resolve the issue.

Recent AI Actions:
• get_container_logs - Found 47 PHP fatal errors
• restart_container - Container restarted but crashed again
• diagnose_deployment - Memory limit exceeded

Recommended Next Steps:
1. Increase PHP memory_limit in container config
2. Check for memory leaks in user's custom plugins
3. Consider upgrading user's plan for more resources

Chat Context:
https://try.direct/admin/support/chats/abc123
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
```

### 2. Tawk.to Live Chat

**Secondary channel when agents are online.**

If a Tawk.to agent is available, the AI will also:
- Open the Tawk.to widget on the user's screen
- Pre-fill context about the issue
- The user can then chat directly with support

---

## Escalation Triggers

The AI escalates in these situations:

| Trigger | Description | Priority |
|---------|-------------|----------|
| **AI stuck** | AI explicitly cannot resolve the issue | Medium |
| **User request** | User asks for human support | High |
| **Repeated failures** | 3+ failed tool calls in sequence | High |
| **Critical errors** | Security issues, data loss risk | Critical |
| **Billing issues** | Payment/subscription problems | Medium |
| **Infrastructure down** | Server unreachable | Critical |

---

## Escalation Fields Explained

### User Information
- **User Email**: Account email for identification
- **User ID**: Database ID for quick lookup
- **Subscription Plan**: Current plan (Free, Starter, Pro, Enterprise)

### Deployment Context
- **Deployment Hash**: Unique identifier (use in admin panel)
- **Stack Type**: What application stack is deployed
- **Cloud Provider**: DigitalOcean, Hetzner, AWS, Linode
- **Server IP**: If available

### Issue Details
- **Summary**: AI-generated description of the problem
- **Recent AI Actions**: What the AI already tried
- **Error Patterns**: Categorized errors found in logs
- **Recommended Steps**: AI suggestions for resolution

### Priority Levels
| Level | Response SLA | Examples |
|-------|--------------|----------|
| **Critical** | 15 minutes | Server down, data loss, security breach |
| **High** | 1 hour | Deployment failed, all containers crashed |
| **Medium** | 4 hours | Single container issues, configuration problems |
| **Low** | 24 hours | General questions, feature requests |

---

## Handling Escalations

### Step 1: Acknowledge

React to the Slack message with ✅ to indicate you're handling it:
```
React with: ✅ (to claim)
```

Then reply in thread:
```
Taking this one. ETA: 15 minutes.
```

### Step 2: Gather Context

1. **Check Admin Panel**: `https://try.direct/admin/users/{user_id}`
   - View full deployment history
   - Check subscription status
   - Review recent activity

2. **Access Deployment**: `https://try.direct/admin/installations/{deployment_hash}`
   - View container statuses
   - Access server logs
   - Check resource usage

3. **Review Chat History**: Click the chat context link in the escalation
   - Understand what user tried
   - See full AI conversation
   - Identify user's exact goal

### Step 3: Diagnose

**Common Issues & Solutions:**

| Issue | Diagnosis | Solution |
|-------|-----------|----------|
| Container crash loop | OOM, config error | Increase limits, fix config |
| Connection refused | Port conflict, firewall | Check ports, security groups |
| SSL not working | DNS propagation, cert issue | Wait for DNS, renew cert |
| Slow performance | Resource exhaustion | Scale up, optimize queries |
| Database errors | Credentials, connection limit | Reset password, increase connections |

### Step 4: Resolve or Escalate Further

**If you can resolve:**
1. Apply the fix
2. Verify with user
3. Update Slack thread with resolution
4. Close the escalation

**If you need to escalate to engineering:**
1. Create a Jira ticket with full context
2. Tag engineering in Slack
3. Update user with ETA
4. Document in the escalation thread

### Step 5: Follow Up

After resolution:
1. Reply to the user in chat (if still online)
2. Send follow-up email summarizing the fix
3. Update internal documentation if it's a new issue pattern
4. Close the Slack thread with ✅ Resolved

---

## Quick Reference Commands

### SSH to User's Server
```bash
# Get server IP from admin panel, then:
ssh root@<server_ip> -i ~/.ssh/trydirect_support
```

### View Container Logs
```bash
# On the server:
docker logs <container_name> --tail 100
docker logs <container_name> --since 1h
```

### Restart Container
```bash
docker-compose -f /opt/stacks/<deployment_hash>/docker-compose.yml restart <service>
```

### Check Resource Usage
```bash
docker stats --no-stream
df -h
free -m
```

### View Environment Variables
```bash
docker exec <container> env | grep -v PASSWORD | grep -v SECRET
```

---

## Common Escalation Patterns

### Pattern 1: Memory Exhaustion

**Symptoms**: Container keeps crashing, OOM errors in logs

**Solution**:
```yaml
# In docker-compose.yml, add:
services:
  app:
    deploy:
      resources:
        limits:
          memory: 512M  # Increase from default
```

### Pattern 2: Database Connection Issues

**Symptoms**: "Connection refused", "Too many connections"

**Solution**:
1. Check database container is running
2. Verify credentials in `.env`
3. Increase `max_connections` if needed
4. Check for connection leaks in app

### Pattern 3: SSL Certificate Problems

**Symptoms**: "Certificate expired", browser security warnings

**Solution**:
```bash
# Force certificate renewal
docker exec nginx certbot renew --force-renewal
docker exec nginx nginx -s reload
```

### Pattern 4: Disk Space Full

**Symptoms**: Write errors, database crashes

**Solution**:
```bash
# Clean up Docker
docker system prune -af
docker volume prune -f

# Check large files
du -sh /var/log/*
```

---

## Escalation Response Templates

### Initial Response (Slack Thread)
```
✅ Taking this escalation.

**User**: {email}
**Issue**: {brief summary}
**Status**: Investigating

Will update in 15 minutes.
```

### Resolution (Slack Thread)
```
✅ **RESOLVED**

**Root Cause**: {what was wrong}
**Fix Applied**: {what you did}
**Verification**: {how you confirmed it's working}

User has been notified.
```

### Further Escalation (Slack Thread)
```
⚠️ **ESCALATING TO ENGINEERING**

This requires infrastructure changes beyond support scope.

**Jira**: INFRA-{number}
**Engineering Contact**: @{name}
**User ETA**: Communicated {timeframe}
```

### User Email Template
```
Subject: TryDirect Support - Issue Resolved

Hi {name},

Your support request has been resolved.

**Issue**: {brief description}
**Resolution**: {what was fixed}

Your {stack_name} deployment should now be working correctly.

If you experience any further issues, please don't hesitate to reach out.

Best regards,
TryDirect Support Team
```

---

## Metrics & Reporting

Track these metrics for escalations:

| Metric | Target | How to Measure |
|--------|--------|----------------|
| Response Time | < 15 min (critical), < 1 hr (high) | Time from escalation to ✅ |
| Resolution Time | < 2 hours average | Time from ✅ to resolved |
| First Contact Resolution | > 70% | Resolved without further escalation |
| User Satisfaction | > 4.5/5 | Post-resolution survey |

---

## FAQ

### Q: What if I can't reproduce the issue?

Ask the user for:
1. Steps to reproduce
2. Browser console logs (for frontend issues)
3. Exact error messages
4. Time when issue occurred

### Q: What if the user is unresponsive?

1. Send follow-up email after 24 hours
2. Leave Slack thread open for 48 hours
3. Close with "No response from user" if still unresponsive

### Q: What if it's a billing issue?

1. Do NOT modify subscriptions directly
2. Escalate to billing team in `#billing`
3. User Service has `/admin/subscriptions` for viewing only

### Q: What if the AI made an error?

1. Document the AI error in the thread
2. Report in `#ai-feedback` channel
3. Include: what AI did wrong, what should have happened

---

## Contacts

| Team | Channel | When to Contact |
|------|---------|-----------------|
| **Engineering** | `#engineering` | Infrastructure issues, bugs |
| **Billing** | `#billing` | Payment, subscription issues |
| **Security** | `#security` | Security incidents, breaches |
| **AI Team** | `#ai-feedback` | AI behavior issues, improvements |

---

## Appendix: Admin Panel Quick Links

- **User Management**: `https://try.direct/admin/users`
- **Installations**: `https://try.direct/admin/installations`
- **Support Chats**: `https://try.direct/admin/support/chats`
- **Server Status**: `https://try.direct/admin/servers`
- **Logs Viewer**: `https://try.direct/admin/logs`
