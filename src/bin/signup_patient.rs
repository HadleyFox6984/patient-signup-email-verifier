use patient_signup_verifier::infrai_client::InfraiClient;
use patient_signup_verifier::patient_signup::{register_patient, PatientSignup};
use std::{env, process::ExitCode};

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("signup failed: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let signup_id = required_arg(&mut args, "signup-id")?;
    let email = required_arg(&mut args, "email")?;
    let password = required_arg(&mut args, "password")?;
    let display_name = required_arg(&mut args, "display-name")?;
    let verification_origin = required_arg(&mut args, "verification-origin")?;
    if args.next().is_some() {
        return Err("too many arguments".into());
    }

    let api_key = env::var("INFRAI_API_KEY")?;
    let infrai = InfraiClient::new(api_key);
    let receipt = register_patient(
        &infrai,
        &PatientSignup {
            signup_id: &signup_id,
            email: &email,
            password: &password,
            display_name: &display_name,
            verification_origin: &verification_origin,
        },
    )
    .await?;

    println!(
        "state={:?} user_id={} message_id={}",
        receipt.state, receipt.user_id, receipt.message_id
    );
    Ok(())
}

fn required_arg(
    args: &mut impl Iterator<Item = String>,
    name: &'static str,
) -> Result<String, Box<dyn std::error::Error>> {
    args.next()
        .ok_or_else(|| format!("missing {name}; run with <signup-id> <email> <password> <display-name> <verification-origin>").into())
}

