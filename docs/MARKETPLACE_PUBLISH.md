# Publishing a Stack to the TryDirect Marketplace

This guide walks creators through publishing a stack to the TryDirect marketplace
so other users can deploy it in one click and you earn on every sale.

---

## Table of Contents

- [Who this is for](#who-this-is-for)
- [What you get](#what-you-get)
- [Two ways to publish](#two-ways-to-publish)
- [Path A: Publish from Stack Builder (UI)](#path-a-publish-from-stack-builder-ui)
- [Path B: Publish from CLI (stacker.yml)](#path-b-publish-from-cli-stackeryml)
- [Required metadata](#required-metadata)
- [Pricing options](#pricing-options)
- [The review process](#the-review-process)
- [After approval](#after-approval)
- [Updating a published template](#updating-a-published-template)
- [Common rejection reasons](#common-rejection-reasons)
- [FAQ](#faq)

---

## Who this is for

You should publish to the marketplace if you have a working Docker Compose stack
or `stacker.yml` that:

- Solves a clear business problem (e.g. "Internal AI Helpdesk", "RAG Knowledge Base")
- Wires multiple services together so buyers don't have to (database + cache + app + LLM, etc.)
- Has been tested end-to-end on at least one cloud provider

Single-container stacks are accepted but multi-service bundles consistently
outperform them in deployments and revenue.

---

## What you get

- **75% revenue share** on every paid deployment, including subscription renewals
- **Monthly Net-30 payouts** via Stripe Connect or PayPal (minimum $50)
- Automatic marketplace promotion: SEO landing page at `/applications/<slug>`,
  inclusion in weekly digests, "Trending" badge if your template gains traction
- A "Deploy with TryDirect" badge you can embed in your GitHub README
- Public deploy count and creator profile

Full payout terms: see `config/docs/MARKETPLACE_PAYOUT_TERMS.md`.

---

## Two ways to publish

| Path | Best for | Effort |
|---|---|---|
| **Stack Builder (UI)** | Stacks you built visually inside TryDirect | Click "Publish to Marketplace" |
| **stacker.yml (CLI)** | Stacks defined in your own repo | `stacker publish` |

Both paths produce the same `StackTemplate` record and follow the same review
process. Pick whichever fits how you author your stack.

---

## Path A: Publish from Stack Builder (UI)

1. Open your stack in **Stack Builder** at `/builder`.
2. Verify it deploys cleanly to a test server.
3. Click **Publish to Marketplace** in the project sidebar.
4. Fill in the publish form:
   - **Name** — a business-oriented name, not just tech ("Client AI Agent Workspace", not "n8n+Qdrant+Ollama")
   - **Short description** — one sentence: what problem does it solve?
   - **Long description** — markdown supported; cover use cases, requirements, customisation
   - **Category** — AI Agents, Data Pipelines, SaaS Starter, Dev Tools, etc.
   - **Tags** — `n8n`, `qdrant`, `ollama`, `supabase`, `postgres`, etc.
   - **License / pricing** — Free, Paid (one-time), or Subscription
   - **Price** (if paid) — USD
   - **Support URL** — GitHub repo, docs site, or contact form
   - **No-secrets confirmation** — required checkbox: confirms you removed all
     embedded credentials before submitting
5. Click **Submit to marketplace**. Your dashboard will show status:
   `In review` → `Approved` or `Rejected (with reason)`.

---

## Path B: Publish from CLI (stacker.yml)

Add a `marketplace` section to your `stacker.yml`, then run `stacker publish`.

### Example `stacker.yml` marketplace block

```yaml
name: ai-helpdesk-starter
version: 1.0.0

# ... your existing app, services, deploy, etc. ...

marketplace:
  publish: true
  display_name: "Internal AI Helpdesk"
  short_description: "Self-hosted AI helpdesk with n8n workflows, Qdrant memory, and Ollama LLM."
  long_description: |
    Deploy a complete internal AI helpdesk stack: n8n handles ticket routing
    and workflow automation, Qdrant stores conversation memory and document
    embeddings, and Ollama serves the local LLM.

    Comes pre-wired with example workflows for common helpdesk patterns.
    Customise via the n8n web UI after deployment.
  category: ai-agents
  tags:
    - n8n
    - qdrant
    - ollama
    - helpdesk
    - rag
  license: paid
  pricing:
    plan_type: one_time   # one_time | subscription | free
    price: 49
    currency: USD
  support_url: https://github.com/your-org/ai-helpdesk-starter
  no_secrets_confirmation: true
```

### Submit

```bash
# From your project root
stacker publish

# Stacker validates stacker.yml, packages the stack definition,
# and submits it to the TryDirect marketplace for review.
```

### Check status

```bash
stacker publish --status

# Shows: in_review | approved | rejected (with reason)
```

---

## Required metadata

| Field | Required | Notes |
|---|---|---|
| Name | Yes | 5-80 chars, business-oriented |
| Short description | Yes | 20-200 chars, one sentence |
| Long description | Yes | Markdown, 100-5000 chars |
| Category | Yes | Must match an existing category code |
| Tags | Yes | 1-10 tags |
| License | Yes | `free`, `paid`, `subscription` |
| Price | If paid/subscription | USD, > 0 |
| Support URL | Yes | Public URL where buyers can reach you |
| No-secrets confirmation | Yes | Must be `true` |

---

## Pricing options

### Free
No revenue, but full marketplace promotion (SEO page, digest inclusion, trending
badges). Good for building reputation and audience.

### One-time
Buyer pays once, deploys as many times as they want for their own use.
You earn 75% of the sale price. Most templates start here.

### Subscription
Buyer pays monthly or yearly. You earn 75% of every renewal cycle, not just
the first sale. Best for templates that ship updates regularly.

You can change pricing on future versions but not retroactively. Buyers of
v1.0.0 keep their original pricing for that version's lifetime.

---

## The review process

| Step | Time | Who |
|---|---|---|
| Submission received | Instant | Auto-confirmation email |
| Initial automated checks | < 1 hour | `stacker.yml` validation, no embedded secrets, no banned services |
| Manual review | 1-3 business days | TryDirect review team |
| Decision | — | Approved → live on `/applications`; Rejected → feedback in dashboard |

Reviewers check:
1. **Deploys cleanly** on a fresh server
2. **No embedded credentials** in env vars, configs, or volumes
3. **No insecure defaults** (e.g. `--api.insecure=true`, `0.0.0.0/0` ACLs, hardcoded passwords)
4. **Metadata accurate** — what the listing claims matches what the stack actually does
5. **Support URL reachable** — opens to a real GitHub/docs/contact page

---

## After approval

Once approved, several things happen automatically:

- **SEO landing page** generated at `/applications/<your-slug>`
- **Social post** to TryDirect Twitter/X: "New on TryDirect: <Template> by @<you>"
- **"You're live!" email** with your sharing kit:
  - Direct deployment link
  - Referral link (tracks deployments and credits earnings)
  - "Deploy with TryDirect" Markdown badge for your README
  - Auto-generated OG social card
- **Inclusion in weekly subscriber digest** for the relevant category

Your dashboard at `/dashboard/marketplace/submissions?tab=earnings` shows real-time:
- Deploy count
- Gross sales
- Your share (75%)
- Pending payout
- Paid history

---

## Updating a published template

To publish a new version:

**UI:** Open the stack in Stack Builder, make changes, click **Publish new version**.

**CLI:**
```bash
# Bump the version in stacker.yml first
stacker publish
```

Each version goes through review. While a new version is under review, the
previous approved version remains live.

Existing buyers automatically get access to all future versions — they don't
re-purchase.

---

## Common rejection reasons

| Reason | Fix |
|---|---|
| Embedded secrets | Replace hardcoded credentials with env vars; use `${VAR}` interpolation |
| Insecure defaults | Disable insecure flags (e.g. `--api.insecure=true`); restrict bind addresses; require passwords |
| Stack doesn't deploy | Test on a fresh server before resubmitting; check `stacker deploy --target local` works clean |
| Vague metadata | Use a specific business-problem name; describe concrete use cases |
| Broken support URL | Make sure the link resolves and points to a real support channel |
| Banned service | Some services aren't allowed (e.g. unconsented crypto miners, malware tools); see `config/docs/MARKETPLACE_ACCEPTABLE_USE.md` |

Rejections include specific feedback in your dashboard. You can revise and
resubmit without limit.

---

## FAQ

**Q: Can I publish a stack that uses paid third-party services (e.g. OpenAI API)?**  
A: Yes. The buyer brings their own API keys via env vars during deployment.
Document required keys in your long description.

**Q: Can I publish a stack that depends on my own SaaS backend?**  
A: Yes, but the buyer must be able to use it without you having access to their
deployment. Document the SaaS integration clearly.

**Q: What happens if I want to take a template offline?**  
A: You can deprecate it any time. Existing buyers keep access; new buyers can
no longer purchase. Earnings from existing subscriptions continue.

**Q: Can I see who deployed my template?**  
A: You see deploy count and aggregated stats, never individual buyer emails or
deployment details (GDPR/privacy).

**Q: How is the 25% platform fee broken down?**  
A: Payment processing (~3%), marketplace hosting and promotion, review
operations, ongoing platform development.

**Q: Can I run a discount or promo code?**  
A: Discount codes are on the roadmap for 2026 H2. Until then, you can change
your template's base price.

---

## Reference

- `stacker.yml` full reference: `docs/STACKER_YML_REFERENCE.md`
- Vendor profile API: `docs/vendor-profile-endpoints-spec.md`
- Payout terms: `config/docs/MARKETPLACE_PAYOUT_TERMS.md`
- Acceptable use policy: `config/docs/MARKETPLACE_ACCEPTABLE_USE.md`
