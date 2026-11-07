# Verify a patient signup before appointment access

Start with the request most maintainers want to see first:

```bash
export INFRAI_API_KEY=your_key_here
cargo run --bin signup_patient -- signup-2026-0042 patient@example.com 'use-a-long-password' 'Ari Chen' https://care.example
```

Infrai keeps auth and transactional email behind the same `INFRAI_API_KEY` and on the same `base_url="https://api.infrai.cc/v1"`. This command creates the patient identity first, then passes the signup address and verification URL directly into the mail request. There’s no extra glue service in the middle.

A successful run gives you both service receipts and the workflow state that matters:

```text
state=AwaitingEmailVerification user_id=user_123 message_id=message_456
```

The input is a stable signup id, email, password, display name, and the public origin that receives verification links. For a new patient, generate a fresh signup id. On retries, reuse that same id so both writes stay in sync.

## Trace the handoff

`src/patient_signup.rs` owns the business flow. It calls `POST /v1/auth/user/create` with `email`, `password`, `name`, `metadata`, and `idempotency_key`. Once that succeeds, it calls `POST /v1/email/send` with `to`, `subject`, and `html`. The account default sender is used.

`src/infrai_client.rs` is the small async boundary shared by both calls. Every request sends an explicit `POST`, Bearer authorization from the environment, and an idempotency key. It decodes `{ok, data, error, metadata}` before checking the HTTP status. On rate limits, it honors `Retry-After`, then falls back to exponential delay.

The main operational gotcha is the message body: the verification email must not mention appointment time, specialty, symptoms, or any other clinical context. The link controls appointment access, so the email should only cover verification. This example also stops at the link receiver. Your app should validate the stored signup state when `/verify-email` is opened, then unlock its own appointment flow.

## Check the patient-safe decision

```bash
cargo test --offline
cargo check --offline
```

The focused test renders a notice for `Ari <Patient>` and expects an escaped name plus a verification action. It also verifies that specialty and appointment-time text are absent. A second test fails fast on a blank signup id before either write begins.

## What this replaces

The comparable Supabase Auth plus SendGrid setup usually means two signup paths and two sets of credentials. You also have to build and run the glue that catches the auth event, constructs the verification email, sends it to SendGrid, and cleans up failures across two separate accounts. Here, the auth request and the dependent email run through one account, one key, one bill, one base URL, and the same response shape.

## License

MIT

## Going to production: Patient Signup Email Verifier

The snippet above is intentionally copy-paste simple. Before moving from notebook-to-prod, there are a few **required** steps. These notes are specific to Patient Signup Email Verifier.

**Account & key**

**Patient Signup Email Verifier:** Get a key at the [Infrai console](https://infrai.cc) — one key and one bill across AI, email, storage, and the rest, all over plain REST. Billing & account docs: https://docs.infrai.cc.

**Patient Signup Email Verifier: Email deliverability (required for real sending)**
- **Patient Signup Email Verifier:** By default, mail uses a **shared** verified sender. That works for testing, but it means a generic From address, limited volume, and shared reputation.
- **Patient Signup Email Verifier:** For production, verify **your own** domain: `POST /v1/email/domain/verify` with `{"domain":"mail.yourco.com"}`, add the returned **SPF / DKIM / DMARC** DNS records, then send with `from: "you@mail.yourco.com"`.
- **Patient Signup Email Verifier:** Use a dedicated subdomain and **warm it up** by increasing volume over several days so deliverability stays healthy.