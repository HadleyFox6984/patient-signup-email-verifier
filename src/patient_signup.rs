use crate::infrai_client::{CreateUser, InfraiClient, InfraiError, SendEmail};
use serde_json::json;
use thiserror::Error;

#[derive(Debug)]
pub struct PatientSignup<'a> {
    pub signup_id: &'a str,
    pub email: &'a str,
    pub password: &'a str,
    pub display_name: &'a str,
    pub verification_origin: &'a str,
}

#[derive(Debug, PartialEq)]
pub struct SignupReceipt {
    pub user_id: String,
    pub message_id: String,
    pub state: SignupState,
}

#[derive(Debug, PartialEq)]
pub enum SignupState {
    AwaitingEmailVerification,
}

#[derive(Debug, Error)]
pub enum SignupError {
    #[error("{0} must not be empty")]
    InvalidInput(&'static str),
    #[error(transparent)]
    Infrai(#[from] InfraiError),
    #[error("email delivery failed ({email}); deleting created user also failed ({cleanup})")]
    Cleanup {
        email: InfraiError,
        cleanup: InfraiError,
    },
}

pub async fn register_patient(
    infrai: &InfraiClient,
    signup: &PatientSignup<'_>,
) -> Result<SignupReceipt, SignupError> {
    validate(signup)?;
    let verification_url = format!(
        "{}/verify-email?signup_id={}",
        signup.verification_origin.trim_end_matches('/'),
        signup.signup_id
    );
    let created = infrai
        .create_user(&CreateUser {
            email: signup.email,
            password: signup.password,
            name: signup.display_name,
            metadata: json!({
                "signup_id": signup.signup_id,
                "workflow": "appointment_access",
                "verification_url": verification_url,
            }),
            idempotency_key: signup.signup_id,
        })
        .await?;

    let html = verification_email(signup.display_name, &verification_url);
    let sent = match infrai
        .send_email(
            &SendEmail {
                to: signup.email,
                subject: "Verify your email for appointment access",
                html: &html,
            },
            &format!("{}-verification-email", signup.signup_id),
        )
        .await
    {
        Ok(sent) => sent,
        Err(email) => {
            if let Err(cleanup) = infrai.delete_user(&created.id).await {
                return Err(SignupError::Cleanup { email, cleanup });
            }
            return Err(email.into());
        }
    };

    Ok(SignupReceipt {
        user_id: created.id,
        message_id: sent.message_id,
        state: SignupState::AwaitingEmailVerification,
    })
}

fn validate(signup: &PatientSignup<'_>) -> Result<(), SignupError> {
    for (name, value) in [
        ("signup_id", signup.signup_id),
        ("email", signup.email),
        ("password", signup.password),
        ("display_name", signup.display_name),
        ("verification_origin", signup.verification_origin),
    ] {
        if value.trim().is_empty() {
            return Err(SignupError::InvalidInput(name));
        }
    }
    Ok(())
}

fn verification_email(name: &str, verification_url: &str) -> String {
    format!(
        "<p>Hello {},</p><p><a href=\"{}\">Verify your email</a> to access appointment workflows.</p><p>If you did not request this, ignore this email.</p>",
        escape_html(name),
        escape_html(verification_url)
    )
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verification_notice_excludes_appointment_details() {
        let html = verification_email("Ari <Patient>", "https://care.example/verify-email?id=visit-42");

        assert!(html.contains("Verify your email"));
        assert!(html.contains("Ari &lt;Patient&gt;"));
        assert!(!html.contains("cardiology"));
        assert!(!html.contains("appointment time"));
    }

    #[test]
    fn empty_signup_id_is_rejected_before_any_write() {
        let signup = PatientSignup {
            signup_id: " ",
            email: "patient@example.com",
            password: "long-local-password",
            display_name: "Ari",
            verification_origin: "https://care.example",
        };

        assert!(matches!(validate(&signup), Err(SignupError::InvalidInput("signup_id"))));
    }
}
