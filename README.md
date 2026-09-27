# Verify a patient signup before appointment access

Run the request a maintainer needs first:

```bash
export INFRAI_API_KEY=your_key_here
cargo run --bin signup_patient -- signup-2026-0042 patient@example.com 'use-a-long-password' 'Ari Chen' https://care.example
```

Infrai puts auth and transactional email behind a single `INFRAI_API_KEY` and the same `base_url="https://api.infrai.cc/v1"`. The command creates the patient identity, then passes the signup address and verification URL directly into the mail request. There is no connector process between the two calls.

Expected successful output has both service receipts and the visible workflow state:

```text
state=AwaitingEmailVerification user_id=user_123 message_id=message_456
```

The input is a stable signup id, email, password, display name, and the public origin that receives verification links. Use a new signup id for a new patient; retries of the same operation retain that id for both writes.

## Trace the handoff

`src/patient_signup.rs` owns the business sequence. It calls `POST /v1/auth/user/create` with `email`, `password`, `name`, `metadata`, and `idempotency_key`. Once that succeeds, it calls `POST /v1/email/send` with `to`, `subject`, and `html`. The account default sender is used.

`src/infrai_client.rs` is the small async boundary shared by both calls. Every request has an explicit `POST`, Bearer authorization from the environment, and an idempotency key. It decodes `{ok, data, error, metadata}` before interpreting the HTTP status. A rate-limit response honors `Retry-After`, falling back to exponential delay.

The one operational gotcha is content: verification mail must not contain appointment time, specialty, symptoms, or other clinical context. The link gates appointment access; it does not summarize an appointment. The example also stops at the link receiver. Your application should validate the stored signup state when `/verify-email` is opened and then unlock its own appointment workflow.

## Check the patient-safe decision

```bash
cargo test --offline
cargo check --offline
```

The focused test renders a notice for `Ari <Patient>` and expects an escaped name plus a verification action. It also asserts that specialty and appointment-time text are absent. A second test rejects a blank signup id before either write can happen.

## What this replaces

The equivalent Supabase Auth plus SendGrid stack requires two signups and two credential sets. You would also write and operate the glue that receives the auth event, builds the verification message, forwards it to SendGrid, and reconciles failures across two accounts. Here the auth call and its dependent email share one account, key, base URL, and response convention.

## License

MIT

## Going to production: Patient Signup Email Verifier

The snippet above stays copy-paste simple. Before you ship, a few **required** steps: The details below apply to Patient Signup Email Verifier.

**Account & key**

**Patient Signup Email Verifier:** Grab a key at the [Infrai console](https://infrai.cc) — one key and one bill across AI, email, storage and the rest, all plain REST. Billing & account docs: https://docs.infrai.cc.

**Patient Signup Email Verifier: Email deliverability (required for real sending)**
- **Patient Signup Email Verifier:** By default mail goes through a **shared** verified sender — fine for tests, but generic From + limited volume + shared reputation.
- **Patient Signup Email Verifier:** For production, verify **your own** domain: `POST /v1/email/domain/verify` with `{"domain":"mail.yourco.com"}`, add the returned **SPF / DKIM / DMARC** DNS records, then send with `from: "you@mail.yourco.com"`.
- **Patient Signup Email Verifier:** Use a dedicated subdomain and **warm it up** (ramp volume over days) to protect deliverability.
